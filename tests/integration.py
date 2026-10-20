#!/usr/bin/env python3
"""Real Docker acceptance test. Run inside WSL after building both images.
Uses unique resources, checks ownership on cleanup, never prunes the daemon.
"""
import base64
import gzip
import hashlib
import http.cookiejar
import io
import json
import math
import os
import socket
import shutil
import struct
import subprocess
import tarfile
import time
import tempfile
import threading
import urllib.error
import urllib.request
import urllib.parse
import http.client
import uuid
import zlib


def docker(*args, check=True, data=None):
    result = subprocess.run(["docker", *args], input=data, capture_output=True, timeout=120)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result.stdout


class WebSocket:
    def __init__(self, port, path, cookie, origin=None):
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=8)
        self.file = self.socket.makefile("rb")
        key = base64.b64encode(os.urandom(16)).decode()
        headers = [f"GET {path} HTTP/1.1", f"Host: 127.0.0.1:{port}", "Upgrade: websocket", "Connection: Upgrade", f"Sec-WebSocket-Key: {key}", "Sec-WebSocket-Version: 13", "Sec-WebSocket-Protocol: binary", f"Origin: {origin or f'http://127.0.0.1:{port}'}", f"Cookie: {cookie}", "", ""]
        self.socket.sendall("\r\n".join(headers).encode())
        status = self.file.readline()
        while self.file.readline() not in (b"\r\n", b""):
            pass
        if b" 101 " not in status:
            self.close()
            raise RuntimeError(f"WebSocket rejected: {status!r}")
        self.pending = b""

    def read(self, size):
        while len(self.pending) < size:
            head = self.file.read(2)
            if len(head) != 2:
                raise EOFError("WebSocket closed")
            opcode, length = head[0] & 15, head[1] & 127
            if length == 126:
                length = struct.unpack("!H", self.file.read(2))[0]
            elif length == 127:
                length = struct.unpack("!Q", self.file.read(8))[0]
            payload = self.file.read(length)
            if opcode == 8:
                raise EOFError("WebSocket closed")
            if opcode == 2:
                self.pending += payload
        result, self.pending = self.pending[:size], self.pending[size:]
        return result

    def send(self, payload):
        mask = os.urandom(4)
        length = len(payload)
        header = bytes([0x82, 0x80 | length]) if length < 126 else bytes([0x82, 0x80 | 126]) + struct.pack("!H", length)
        self.socket.sendall(header + mask + bytes(v ^ mask[i % 4] for i, v in enumerate(payload)))

    def assert_closed(self):
        self.socket.settimeout(3)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            try:
                self.read(4096)
            except (EOFError, ConnectionResetError):
                return
        raise AssertionError("authenticated WebSocket survived logout")

    def close(self):
        self.file.close()
        self.socket.close()


tag = uuid.uuid4().hex[:12]
manager = f"bilskirnir-test-{tag}"
volume = f"bilskirnir-test-data-{tag}"
share = f"bilskirnir-test-share-{tag}"
clone_share = f"bilskirnir-test-clone-{tag}"
files_volume = f"bilskirnir-test-files-{tag}"
restored_volume = f"bilskirnir-test-restored-{tag}"
streamed_volume = f"bilskirnir-test-streamed-{tag}"
cut_volume = f"bilskirnir-test-cut-{tag}"
interrupted_volume = f"bilskirnir-test-interrupted-{tag}"
copied_volume = f"bilskirnir-test-copied-{tag}"
mcp_copy_volume = f"bilskirnir-test-mcp-copy-{tag}"
device_volume = f"bilskirnir-test-device-{tag}"
device_copy_volume = f"bilskirnir-test-device-copy-{tag}"
linked_volume = f"bilskirnir-test-linked-{tag}"
installs_volume = f"bilskirnir-test-installs-{tag}"
cache_volume = f"bilskirnir-test-cache-{tag}"
settings_volume = f"bilskirnir-test-settings-{tag}"
absent_volume = f"bilskirnir-test-absent-{tag}"
outside_volume = f"bilskirnir-test-outside-{tag}"
password = uuid.uuid4().hex
mcp_token = uuid.uuid4().hex + uuid.uuid4().hex
container_ids = []
image_refs = []
# Volumes the manager names after this run's containers, which exist only once they are created.
own_volumes = []
foreign = f"bilskirnir-test-foreign-{tag}"
holder = f"bilskirnir-test-holder-{tag}"
port = None
cookies = http.cookiejar.CookieJar()
client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookies))


def api(path, body=None, method=None, raw=False, anonymous=False):
    if path == "images/build" and body and "parts" in body:
        # Every later part's layer builds on the first, so marking it keeps the run's own.
        first, *rest = body["parts"]
        body = {**body, "parts": [{**first, "script": f"# acceptance run {tag}\n" + first["script"]}, *rest]}
    elif path == "images/build" and body:
        body = {**body, "script": f"# acceptance run {tag}\n" + body["script"]}
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/{path}", data=None if body is None else json.dumps(body).encode(), method=method, headers={"Content-Type": "application/json", "X-Bilskirnir-Request": "1"})
    opener = urllib.request.build_opener() if anonymous else client
    with opener.open(request, timeout=130) as response:
        data = response.read()
        return data if raw else (json.loads(data) if data else None)


def png(width, height):
    """A small RGBA PNG, the format image icons are kept in."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + b"\x61\x52\xc4\xff" * width for _ in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


# Runs in a GPU desktop under gpu-run: an OpenGL window that the window manager maximizes while it
# takes its events with XCheckIfEvent, as Godot and SDL 2.0.18+ do. It prints the window's size and
# the size VirtualGL renders it at, which before VirtualGL 3.1.5 stayed the size it was created with.
GL_RESIZE_PROBE = r'''
import subprocess, time
from ctypes import CDLL, CFUNCTYPE, POINTER, Structure, Union, byref, c_char_p, c_int, c_long, c_uint, c_ulong, c_void_p
class Visual(Structure): _fields_ = [("visual", c_void_p), ("id", c_ulong), ("screen", c_int), ("depth", c_int)]
class Attributes(Structure): _fields_ = [("unused", c_ulong * 4), ("gravity", c_int * 3), ("planes", c_ulong * 2), ("save_under", c_int), ("event_mask", c_long), ("dont_propagate", c_long), ("override", c_int), ("colormap", c_ulong), ("cursor", c_ulong)]
class Configure(Structure): _fields_ = [("type", c_int), ("serial", c_ulong), ("send_event", c_int), ("display", c_void_p), ("event", c_ulong), ("window", c_ulong), ("x", c_int), ("y", c_int), ("width", c_int), ("height", c_int)]
class Event(Union): _fields_ = [("type", c_int), ("configure", Configure), ("pad", c_long * 24)]
x11, gl = CDLL("libX11.so.6"), CDLL("libGL.so.1")
def sig(lib, name, restype, *argtypes):
    function = getattr(lib, name)
    function.restype, function.argtypes = restype, list(argtypes)
match_all = CFUNCTYPE(c_int, c_void_p, POINTER(Event), c_void_p)(lambda display, event, arg: 1)
sig(x11, "XOpenDisplay", c_void_p, c_char_p)
sig(x11, "XRootWindow", c_ulong, c_void_p, c_int)
sig(x11, "XCreateColormap", c_ulong, c_void_p, c_ulong, c_void_p, c_int)
sig(x11, "XCreateWindow", c_ulong, c_void_p, c_ulong, c_int, c_int, c_uint, c_uint, c_uint, c_int, c_uint, c_void_p, c_ulong, POINTER(Attributes))
sig(x11, "XMapWindow", c_int, c_void_p, c_ulong)
sig(x11, "XCheckIfEvent", c_int, c_void_p, POINTER(Event), type(match_all), c_void_p)
sig(gl, "glXChooseVisual", POINTER(Visual), c_void_p, c_int, POINTER(c_int))
sig(gl, "glXCreateContext", c_void_p, c_void_p, POINTER(Visual), c_void_p, c_int)
sig(gl, "glXMakeCurrent", c_int, c_void_p, c_ulong, c_void_p)
sig(gl, "glXSwapBuffers", None, c_void_p, c_ulong)
sig(gl, "glXQueryDrawable", None, c_void_p, c_ulong, c_int, POINTER(c_uint))
display = x11.XOpenDisplay(None)
visual = gl.glXChooseVisual(display, 0, (c_int * 3)(4, 5, 0))  # GLX_RGBA, GLX_DOUBLEBUFFER
root = x11.XRootWindow(display, 0)
attributes = Attributes(event_mask=1 << 17, colormap=x11.XCreateColormap(display, root, visual.contents.visual, 0))  # StructureNotifyMask
window = x11.XCreateWindow(display, root, 0, 0, 400, 300, 0, visual.contents.depth, 1, visual.contents.visual, 1 << 13 | 1 << 11, byref(attributes))
x11.XMapWindow(display, window)
gl.glXMakeCurrent(display, window, gl.glXCreateContext(display, visual, None, 1))
width, height, event, mapped, maximized, start = 400, 300, Event(), False, False, time.monotonic()
while time.monotonic() - start < 5:
    while x11.XCheckIfEvent(display, byref(event), match_all, None):
        if event.configure.window == window and event.type == 19:  # MapNotify
            mapped = True
        if event.configure.window == window and event.type == 22:  # ConfigureNotify
            width, height = event.configure.width, event.configure.height
    if mapped and not maximized:
        subprocess.run(["wmctrl", "-i", "-r", hex(window), "-b", "add,maximized_vert,maximized_horz"], check=True)
        maximized = True
    gl.glViewport(0, 0, width, height)
    gl.glClear(0x4000)  # GL_COLOR_BUFFER_BIT
    gl.glXSwapBuffers(display, window)
    time.sleep(0.02)
drawable = [c_uint(), c_uint()]
gl.glXQueryDrawable(display, window, 0x801D, byref(drawable[0]))  # GLX_WIDTH
gl.glXQueryDrawable(display, window, 0x801E, byref(drawable[1]))  # GLX_HEIGHT
print(f"window={width}x{height} drawable={drawable[0].value}x{drawable[1].value}")
'''


def wait_for(fn, seconds=90):
    deadline, error = time.monotonic() + seconds, None
    while time.monotonic() < deadline:
        try:
            value = fn()
            if value:
                return value
        except Exception as exc:
            error = exc
        time.sleep(0.5)
    raise RuntimeError(f"Timed out: {error}")


def job(response):
    result = wait_for(lambda: (j if (j := api(f"jobs/{response['job_id']}"))["status"] not in ("queued", "running") else None), 180)
    assert result["status"] == "completed", result
    if result["result"] and "reference" in result["result"] and result["result"].get("kind") != "catalog":
        image_refs.append(result["result"]["reference"])
    return result["result"]


def start_manager():
    docker("run", "-d", "--name", manager, "--label", "bilskirnir.manager=true", "-e", f"MANAGER_CONTAINER={manager}", "-e", "AUTH_MODE=password", "-e", f"ADMIN_PASSWORD={password}", "-e", "MCP_ENABLED=true", "-e", f"MCP_TOKEN={mcp_token}", "-e", "GPU_ADAPTER=NVIDIA", "-v", "/var/run/docker.sock:/var/run/docker.sock", "-v", f"{volume}:/data", "-p", f"127.0.0.1:{port or ''}:8080", "bilskirnir/manager:local")
    return int(docker("port", manager, "8080/tcp").decode().strip().rsplit(":", 1)[1])


def login():
    wait_for(lambda: api("session", anonymous=True))
    api("login", {"username": "admin", "password": password})


def mcp(method, params=None, token=mcp_token):
    request = urllib.request.Request(f"http://127.0.0.1:{port}/mcp", data=json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params or {}}).encode(), headers={"Content-Type":"application/json","Accept":"application/json, text/event-stream","MCP-Protocol-Version":"2025-11-25","Authorization":f"Bearer {token}"})
    with urllib.request.urlopen(request, timeout=120) as response:
        result = json.loads(response.read())
    assert "error" not in result, result
    return result["result"]


def tool(name, arguments=None, allow_error=False):
    result = mcp("tools/call", {"name":name,"arguments":arguments or {}})
    if not allow_error:
        assert not result.get("isError"), result
    return result


def stdio_tool(name, arguments=None):
    messages = [{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"stdio-acceptance","version":"1"}}},
                {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments or {}}}]
    result = subprocess.run(["docker","exec","-i",manager,"python3","/app/mcp_bridge.py"], input=("\n".join(json.dumps(m) for m in messages)+"\n").encode(),capture_output=True,timeout=130)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    replies = {m["id"]:m for line in result.stdout.splitlines() if (m:=json.loads(line))}
    assert replies[1]["result"]["protocolVersion"] == "2025-11-25"
    response = replies[2]["result"]
    assert not response.get("isError"), response
    return response


def endpoint_post(name, token, message, session=None):
    headers = {"Content-Type":"application/json","Accept":"application/json, text/event-stream","MCP-Protocol-Version":"2025-11-25","Authorization":f"Bearer {token}"}
    if session:
        headers["Mcp-Session-Id"] = session
    request = urllib.request.Request(f"http://127.0.0.1:{port}/mcp/{name}", data=json.dumps(message).encode(), headers=headers)
    with urllib.request.urlopen(request, timeout=180) as response:
        body = response.read().decode()
        replies = [json.loads(line[5:]) for line in body.splitlines() if line.startswith("data:") and line[5:].strip()]
        return response.headers.get("Mcp-Session-Id"), (json.loads(body) if body.strip().startswith("{") else (replies[-1] if replies else None))


def endpoint_session(name, token):
    session, reply = endpoint_post(name, token, {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"endpoint-acceptance","version":"1"}}})
    assert reply["result"]["capabilities"]["tools"]["listChanged"] is True, reply
    endpoint_post(name, token, {"jsonrpc":"2.0","method":"notifications/initialized"}, session)
    return session


def endpoint_call(name, token, session, method, params=None):
    _, reply = endpoint_post(name, token, {"jsonrpc":"2.0","id":2,"method":method,"params":params or {}}, session)
    assert "error" not in reply and not reply["result"].get("isError"), reply
    return reply["result"]


# A desktop MCP server on stdio: one echo tool that reports who runs it.
ECHO_SERVER = r'''import json, os, sys
for line in sys.stdin:
    message = json.loads(line)
    if "id" not in message:
        continue
    method = message["method"]
    if method == "initialize":
        result = {"protocolVersion": message["params"]["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "echo", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "Echo text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}]}
    elif method == "tools/call":
        text = "echo: %s uid=%d display=%s" % (message["params"]["arguments"]["text"], os.getuid(), os.environ.get("DISPLAY"))
        result = {"content": [{"type": "text", "text": text}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": method}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}), flush=True)
'''
# The same tool from a Streamable HTTP server that listens on 127.0.0.1 only and needs a header.
ECHO_HTTP_SERVER = r'''import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
class Handler(BaseHTTPRequestHandler):
    def reply(self, status, body=None, session=False):
        data = json.dumps(body).encode() if body is not None else b""
        self.send_response(status)
        if body is not None:
            self.send_header("Content-Type", "application/json")
        if session:
            self.send_header("Mcp-Session-Id", "acceptance")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_POST(self):
        if self.headers.get("X-Test") != "yes":
            return self.reply(401, {"error": "header missing"})
        message = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if "id" not in message:
            return self.reply(202)
        method = message["method"]
        if method == "initialize":
            result = {"protocolVersion": message["params"]["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "echo-http", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": [{"name": "echo", "description": "Echo text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}]}
        else:
            result = {"content": [{"type": "text", "text": "echo: %s header=%s" % (message["params"]["arguments"]["text"], self.headers.get("X-Test"))}]}
        self.reply(200, {"jsonrpc": "2.0", "id": message["id"], "result": result}, session=method == "initialize")
    def do_GET(self):
        self.reply(405)
    def do_DELETE(self):
        self.reply(200)
ThreadingHTTPServer(("127.0.0.1", 8931), Handler).serve_forever()
'''


def mcp_job(response, success=True):
    jid = response["structuredContent"]["job_id"]
    result = wait_for(lambda: (j if (j := tool("get_job", {"job_id":jid})["structuredContent"])["status"] not in ("queued","running") else None), 180)
    assert result["status"] == ("completed" if success else "failed"), result
    if success and result["result"] and result["result"].get("reference") and result["result"].get("kind") != "catalog":
        image_refs.append(result["result"]["reference"])
    return result


def upload(eid, name, data, destination="/home/desktop/Downloads", replace=False):
    query = urllib.parse.urlencode({"destination":destination,"name":name,"size":len(data),"replace":str(replace).lower()})
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/containers/{eid}/files?{query}",data=data,headers={"Content-Type":"application/octet-stream","X-Bilskirnir-Request":"1"})
    with client.open(request,timeout=120) as response:
        return json.loads(response.read())


def transfer_link(arguments):
    return tool("create_transfer", arguments)["structuredContent"]


def without_session(url, data=None, method=None):
    """A request with no cookie, no X-Bilskirnir-Request and no bearer token, as a shell sends a link."""
    request = urllib.request.Request(f"http://127.0.0.1:{port}{url}", data=data, method=method)
    return urllib.request.build_opener().open(request, timeout=130)


def import_archive(path, **query):
    size = os.path.getsize(path)
    query = urllib.parse.urlencode({"size": size, **query})
    with open(path, "rb") as body:
        request = urllib.request.Request(f"http://127.0.0.1:{port}/api/images/import?{query}", data=body, method="POST", headers={"Content-Type": "application/octet-stream", "Content-Length": str(size), "X-Bilskirnir-Request": "1"})
        with client.open(request, timeout=1800) as response:
            return json.loads(response.read())


def synthetic_archive(path, labels, names):
    """A one-layer image saved as Docker 25 does, naming `names` in every place Docker reads names."""
    layer_buffer = io.BytesIO()
    with tarfile.open(fileobj=layer_buffer, mode="w") as layer_tar:
        content = f"archive acceptance {tag} {sorted(labels.items())}".encode()
        member = tarfile.TarInfo("bilskirnir-archive-test")
        member.size = len(content)
        layer_tar.addfile(member, io.BytesIO(content))
    layer = layer_buffer.getvalue()
    digest = lambda data: hashlib.sha256(data).hexdigest()
    config = json.dumps({"architecture": "amd64", "os": "linux", "config": {"Labels": labels}, "rootfs": {"type": "layers", "diff_ids": [f"sha256:{digest(layer)}"]}, "history": [{"created_by": "acceptance"}]}).encode()
    manifest = json.dumps({"schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json", "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": f"sha256:{digest(config)}", "size": len(config)}, "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": f"sha256:{digest(layer)}", "size": len(layer)}]}).encode()
    repository, version = names[0].rsplit(":", 1)
    index = json.dumps({"schemaVersion": 2, "mediaType": "application/vnd.oci.image.index.v1+json", "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": f"sha256:{digest(manifest)}", "size": len(manifest), "annotations": {"io.containerd.image.name": names[0], "org.opencontainers.image.ref.name": version}}]}).encode()
    files = [
        (f"blobs/sha256/{digest(layer)}", layer),
        (f"blobs/sha256/{digest(config)}", config),
        (f"blobs/sha256/{digest(manifest)}", manifest),
        ("index.json", index),
        ("manifest.json", json.dumps([{"Config": f"blobs/sha256/{digest(config)}", "RepoTags": names, "Layers": [f"blobs/sha256/{digest(layer)}"]}]).encode()),
        ("oci-layout", b'{"imageLayoutVersion":"1.0.0"}'),
        ("repositories", json.dumps({repository: {version: digest(config)}}).encode()),
    ]
    with gzip.open(path, "wb") as out, tarfile.open(fileobj=out, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, data in files:
            member = tarfile.TarInfo(name)
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))


def ready(container):
    def check():
        info = json.loads(docker("inspect", container))[0]
        if info["State"].get("Health", {}).get("Status") == "healthy":
            return True
        return None
    try:
        wait_for(check)
    except Exception:
        print(docker("logs", container, check=False).decode(errors="replace"))
        result = subprocess.run(["docker", "logs", container], capture_output=True)
        print(result.stderr.decode(errors="replace"))
        raise


def machine(container):
    """The hostname, machine IDs and interface MAC addresses the desktop itself reports."""
    script = 'echo "h $(hostname)"; echo "i $(cat /etc/machine-id)"; echo "i $(cat /var/lib/dbus/machine-id)"; for i in /sys/class/net/eth*; do echo "m $(cat $i/address)"; done'
    lines = [line.split(" ", 1) for line in docker("exec", container, "sh", "-c", script).decode().splitlines()]
    values = lambda kind: [value.strip() for key, value in lines if key == kind and value.strip()]
    # An empty file, as /etc/machine-id is in the XFCE image, is left out.
    return {"hostname": values("h")[0], "machine_ids": values("i"), "macs": sorted(values("m"))}


def recorded(spec, interfaces=1):
    """What `machine` reports for a container whose record holds `spec`."""
    return {"hostname": spec["hostname"], "machine_ids": [spec["machine_id"]] * 2, "macs": [spec["mac_address"]] * interfaces}


try:
    port = start_manager()
    login()
    # GPU usage helpers of another manager on this Docker are not this run's to judge.
    foreign_helpers = set(docker("ps", "-q", "--filter", "label=bilskirnir.usage-helper=true").decode().split())
    try:
        api("containers", anonymous=True)
        raise AssertionError("anonymous API access accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 401
    print("PASS password login and API access checks", flush=True)
    try:
        mcp("tools/list", token="wrong")
        raise AssertionError("MCP accepted the wrong token")
    except urllib.error.HTTPError as error:
        assert error.code == 401
    assert mcp("initialize", {"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"acceptance","version":"1"}})["serverInfo"]["name"] == "bilskirnir"
    assert len(mcp("tools/list")["tools"]) == 75
    assert tool("get_system")["structuredContent"]["connected"]
    assert stdio_tool("get_system")["structuredContent"]["connected"]
    print("PASS MCP authentication, negotiation and discovery", flush=True)
    assert api("system")["connected"]
    catalog = [i for i in api("images") if i["kind"] == "catalog"]
    assert all(i["available"] for i in catalog), catalog
    assert {i["desktop"]["session"] for i in catalog} == {"xfce", "gnome"}, catalog
    # The catalog leads the image list, before any saved image.
    assert [i["kind"] for i in api("images")[:len(catalog)]] == ["catalog"] * len(catalog)
    assert [i["id"] for i in tool("list_images")["structuredContent"]["items"] if i["kind"] == "catalog"] == [i["id"] for i in catalog]
    xfce = next(i for i in catalog if i["desktop"]["session"] == "xfce")
    gnome = next(i for i in catalog if i["desktop"]["session"] == "gnome")
    # A catalog image build through the manager must work from its bundled Docker context.
    assert mcp_job(tool("build_catalog_image", {"image_id":"ubuntu-xfce"}))["result"]["id"] == "ubuntu-xfce"
    print("PASS distinct catalog images and built-in image preparation", flush=True)
    # A container created from a catalog image the host does not have downloads the published
    # image in its creation job, which reports byte progress while it runs. Only the catalog tag
    # is moved aside, never the local build; it is restored, and only an image this phase pulled
    # is removed.
    xfce_ref = xfce["reference"]
    # A release manager downloads the catalog image published with it and tags that as the reference.
    release = docker("inspect", "--format", '{{index .Config.Labels "bilskirnir.release"}}', manager).decode().strip()
    xfce_source = xfce_ref if release in ("", "<no value>") else f"{xfce_ref.rsplit(':', 1)[0]}:{release}"
    local_xfce = docker("image", "inspect", "--format", "{{.Id}}", xfce_ref).decode().strip()
    known_images = set(docker("image", "ls", "-a", "-q", "--no-trunc").decode().split())
    xfce_backup = f"bilskirnir-test-xfce-backup:{tag}"
    docker("image", "tag", xfce_ref, xfce_backup)
    docker("image", "rm", xfce_ref)
    # A release tag already on the host is moved aside the same way, so the download is a real one.
    local_source = docker("image", "inspect", "--format", "{{.Id}}", xfce_source, check=False).decode().strip() if xfce_source != xfce_ref else ""
    source_backup = f"bilskirnir-test-xfce-release-backup:{tag}"
    if local_source:
        docker("image", "tag", xfce_source, source_backup)
        docker("image", "rm", xfce_source)
    try:
        missing = next(i for i in api("images") if i["id"] == "ubuntu-xfce")
        assert not missing["available"] and not missing["downloaded"] and missing["image_id"] is None, missing
        created = api("containers", {"name": "Lazy catalog download", "image": xfce_ref, "gpu": False, "internet": False})
        # The pages show the container under this name while its record does not exist yet.
        listed = next(j for j in api("jobs") if j["id"] == created["job_id"])
        assert listed["name"] == "Lazy catalog download", listed
        samples, deadline = [], time.monotonic() + 1200
        while (state := api(f"jobs/{created['job_id']}"))["status"] in ("queued", "running"):
            assert time.monotonic() < deadline, state["log"]
            if state.get("progress"):
                samples.append(state["progress"])
            time.sleep(0.5)
        assert state["status"] == "completed" and state["name"] == "Lazy catalog download", state
        lazy_env = state["result"]["container_id"]
        container_ids.append(lazy_env)
        assert "progress" not in state and f"from {xfce_source}." in state["log"], state
        assert "Status: Downloaded newer image" in state["log"], state["log"]
        if xfce_source != xfce_ref:
            assert f"Tagged it as {xfce_ref}." in state["log"], state["log"]
            assert not docker("image", "ls", "-q", xfce_source).strip(), "the release tag was left behind"
        assert any(p["layers"] and p["total"] is not None for p in samples), samples
        assert [p["current"] for p in samples] == sorted(p["current"] for p in samples), samples
        assert all(p["total"] is None or p["current"] <= p["total"] for p in samples), samples
        assert job(api(f"containers/{lazy_env}", method="DELETE"))["deleted"] == lazy_env
        container_ids.remove(lazy_env)
    finally:
        pulled_xfce = docker("image", "inspect", "--format", "{{.Id}}", xfce_ref, check=False).decode().strip()
        docker("image", "tag", local_xfce, xfce_ref)
        docker("image", "rm", xfce_backup)
        if local_source:
            docker("image", "tag", local_source, xfce_source)
            docker("image", "rm", source_backup)
        if pulled_xfce and pulled_xfce != local_xfce and pulled_xfce not in known_images:
            image_refs.append(pulled_xfce)
    assert docker("image", "inspect", "--format", "{{.Id}}", xfce_ref).decode().strip() == local_xfce
    print(f"PASS missing catalog image downloads from {xfce_source} on creation with byte progress, named in the job list", flush=True)
    api("volumes", {"name": share})
    api("volumes", {"name": clone_share})
    # A volume's files are reachable without a container, as the desktop user.
    def refused(path, body=None, method=None):
        try:
            api(path, body, method)
        except urllib.error.HTTPError as error:
            return error.code
        raise AssertionError(f"{path} was accepted")
    api("volumes", {"name": files_volume})
    files = f"volumes/{files_volume}/files"
    assert api(f"{files}/list", {"path": "/"})["items"] == []
    api(f"{files}/mkdir", {"path": "/notes"})
    assert refused(f"{files}/mkdir", {"path": "/notes"}) == 409
    api(f"{files}/write", {"path": "/notes/hello.txt", "data": "Hello volume\n"})
    assert refused(f"{files}/write", {"path": "/notes/hello.txt", "data": "again"}) == 409
    assert api(f"{files}/read", {"path": "/notes/hello.txt"})["data"] == "Hello volume\n"
    owners = docker("run", "--rm", "--network", "none", "--mount", f"type=volume,src={files_volume},dst=/v,readonly", "--entrypoint", "stat", "bilskirnir/manager:local", "-c", "%u:%g", "/v", "/v/notes", "/v/notes/hello.txt").decode().split()
    assert owners == ["1001:1001"] * 3, owners
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/{files}?" + urllib.parse.urlencode({"destination": "/notes", "name": "blob.bin", "size": 3}), data=b"\x00\x01\x02", method="POST", headers={"Content-Type": "application/octet-stream", "X-Bilskirnir-Request": "1"})
    with client.open(request, timeout=130) as response:
        assert json.loads(response.read())["path"] == "/notes/blob.bin"
    with client.open(f"http://127.0.0.1:{port}/api/{files}/download?" + urllib.parse.urlencode({"path": "/notes/blob.bin"}), timeout=130) as response:
        assert response.read() == b"\x00\x01\x02"
    api(f"{files}/move", {"source": "/notes/hello.txt", "destination": "/notes/renamed.txt"})
    # A folder of a volume downloads as a .tar.gz of itself, as a job like an export.
    with client.open(f"http://127.0.0.1:{port}/api/{files}/download?" + urllib.parse.urlencode({"path": "/notes"}), timeout=130) as response:
        notes_job = response.headers["X-Job-Id"]
        notes = response.read()
    assert job({"job_id": notes_job})["bytes"] == len(notes)
    with tarfile.open(fileobj=io.BytesIO(notes), mode="r:gz") as archive:
        assert sorted(archive.getnames()) == ["notes", "notes/blob.bin", "notes/renamed.txt"], archive.getnames()
        assert archive.extractfile("notes/renamed.txt").read() == b"Hello volume\n"
    # A volume leaves as an ordinary .tar.gz and comes back into a new volume, never over one.
    with client.open(f"http://127.0.0.1:{port}/api/volumes/{files_volume}/archive", timeout=130) as response:
        export_job = response.headers["X-Job-Id"]
        exported_volume = response.read()
    assert job({"job_id": export_job})["bytes"] == len(exported_volume)
    with tarfile.open(fileobj=io.BytesIO(exported_volume), mode="r:gz") as archive:
        assert {member.name for member in archive.getmembers()} >= {"./notes/renamed.txt", "./notes/blob.bin"}
    def import_volume(name, data):
        request = urllib.request.Request(f"http://127.0.0.1:{port}/api/volumes/import?" + urllib.parse.urlencode({"name": name, "size": len(data)}), data=data, method="POST", headers={"Content-Type": "application/octet-stream", "X-Bilskirnir-Request": "1"})
        with client.open(request, timeout=130) as response:
            return json.loads(response.read())
    try:
        import_volume(files_volume, exported_volume); raise AssertionError("import replaced a volume")
    except urllib.error.HTTPError as error:
        assert error.code == 409
    assert job(import_volume(restored_volume, exported_volume))["name"] == restored_volume
    assert api(f"volumes/{restored_volume}/files/read", {"path": "/notes/renamed.txt"})["data"] == "Hello volume\n"
    restored_owners = docker("run", "--rm", "--network", "none", "--mount", f"type=volume,src={restored_volume},dst=/v,readonly", "--entrypoint", "stat", "bilskirnir/manager:local", "-c", "%u:%g", "/v", "/v/notes/blob.bin").decode().split()
    assert restored_owners == ["1001:1001"] * 2, restored_owners
    # A broken archive leaves no volume behind.
    broken = f"bilskirnir-test-broken-{tag}"
    try:
        started = import_volume(broken, exported_volume[: len(exported_volume) // 2])
    except urllib.error.HTTPError as error:
        assert error.code == 400, error.code
    else:
        finished = wait_for(lambda: (j if (j := api(f"jobs/{started['job_id']}"))["status"] not in ("queued", "running") else None), 180)
        assert finished["status"] == "failed", finished
    assert json.loads(docker("volume", "inspect", broken, check=False) or b"[]") == []
    # An archive streamed as it is made, as `tar -c | curl -T -` sends it, arrives in chunks and
    # without a size. One whose connection closes before the last chunk, or which stops at a file
    # boundary as a sending tar that failed leaves it, creates no volume.
    def stream_volume(name, data, cut=False):
        def chunks():
            # A dropped upload stops inside its second header, before any closing block.
            sent = data[:1000] if cut else data
            for start in range(0, len(sent), 4096):
                yield sent[start:start + 4096]
            if cut:
                raise ConnectionAbortedError("the acceptance test drops the upload")
        request = urllib.request.Request(f"http://127.0.0.1:{port}/api/volumes/import?" + urllib.parse.urlencode({"name": name}), data=chunks(), method="POST", headers={"Content-Type": "application/octet-stream", "X-Bilskirnir-Request": "1"})
        with client.open(request, timeout=130) as response:
            return json.loads(response.read())
    plain_volume = gzip.decompress(exported_volume)
    started = stream_volume(streamed_volume, plain_volume)
    streamed = wait_for(lambda: (j if (j := api(f"jobs/{started['job_id']}"))["status"] not in ("queued", "running") else None), 180)
    assert streamed["status"] == "completed" and "of unknown size" in streamed["log"], streamed
    assert api(f"volumes/{streamed_volume}/files/read", {"path": "/notes/renamed.txt"})["data"] == "Hello volume\n"
    with tarfile.open(fileobj=io.BytesIO(plain_volume)) as archive:
        boundary = archive.getmembers()[-1].offset
    try:
        stream_volume(cut_volume, plain_volume[:boundary]); raise AssertionError("an archive without its closing blocks was accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 400 and "end-of-archive" in json.loads(error.read())["error"], error.code
    try:
        stream_volume(interrupted_volume, plain_volume, cut=True); raise AssertionError("the dropped upload was answered")
    except urllib.error.URLError:
        pass
    dropped = wait_for(lambda: next((j for j in api("jobs") if j["resource"] == f"volume:{interrupted_volume}" and j["status"] not in ("queued", "running")), None), 180)
    assert dropped["status"] == "failed", dropped
    assert "The upload was interrupted" in api(f"jobs/{dropped['id']}")["log"]
    for name in [cut_volume, interrupted_volume]:
        assert json.loads(docker("volume", "inspect", name, check=False) or b"[]") == [], name
    listed = api(f"{files}/list", {"path": "/notes"})
    assert listed["path"] == "/notes" and sorted(i["name"] for i in listed["items"]) == ["blob.bin", "renamed.txt"], listed
    assert refused(f"{files}/delete", {"path": "/notes"}) == 400
    api(f"{files}/delete", {"path": "/notes", "recursive": True})
    assert api(f"{files}/list", {"path": "/"})["items"] == []
    assert refused(f"{files}/delete", {"path": "/", "recursive": True}) == 400
    # The volume's root cannot be escaped, a missing volume is not created and the manager's
    # own data volume stays closed.
    assert refused(f"{files}/list", {"path": "/../etc"}) == 400
    missing = f"bilskirnir-test-missing-{tag}"
    assert refused(f"volumes/{missing}/files/list", {"path": "/"}) == 404
    assert json.loads(docker("volume", "inspect", missing, check=False) or b"[]") == []
    assert refused(f"volumes/{volume}/files/list", {"path": "/"}) == 403
    assert not docker("ps", "-aq", "--filter", "label=bilskirnir.volume-helper=true").strip()
    print("PASS volume files and archives go through a removed helper as the desktop user", flush=True)
    # A copy pipes the export helper into the import helper. It has the source's tree, symlinks
    # and modes, a file only root may read included, and every file belongs to the desktop user.
    # An existing target, a missing source and the manager's own volume on either side are
    # refused, and a copy that fails, here on a device file, leaves no volume.
    def in_volume(name, *command):
        return docker("run", "--rm", "--network", "none", "--mount", f"type=volume,src={name},dst=/v", "--entrypoint", command[0], "bilskirnir/manager:local", *command[1:])
    def tree(name):
        return in_volume(name, "sh", "-c", "cd /v && find . -printf '%p %m %y %l\\n' | sort").decode()
    in_volume(restored_volume, "sh", "-c", "chmod 664 /v/notes/renamed.txt && ln -s notes/renamed.txt /v/link && printf secret > /v/root-only && chmod 600 /v/root-only")
    copied = job(api(f"volumes/{restored_volume}/copy", {"name": copied_volume}))
    assert copied["source"] == restored_volume and copied["name"] == copied_volume and copied["bytes"] > 0, copied
    assert tree(copied_volume) == tree(restored_volume), tree(copied_volume)
    assert set(in_volume(copied_volume, "find", "/v", "-printf", "%u:%g\\n").decode().split()) == {"1001:1001"}
    assert in_volume(copied_volume, "cat", "/v/root-only") == b"secret"
    assert mcp_job(tool("copy_volume", {"volume": restored_volume, "name": mcp_copy_volume}))["result"]["name"] == mcp_copy_volume
    assert tree(mcp_copy_volume) == tree(restored_volume)
    never = f"bilskirnir-test-never-{tag}"
    for source, target, status in [(restored_volume, copied_volume, 409), (missing, never, 404), (volume, never, 403), (restored_volume, volume, 403)]:
        assert refused(f"volumes/{source}/copy", {"name": target}) == status, (source, target)
    for name in [missing, never]:
        assert json.loads(docker("volume", "inspect", name, check=False) or b"[]") == [], name
    api("volumes", {"name": device_volume})
    in_volume(device_volume, "mknod", "/v/null", "c", "1", "3")
    failed_copy = api(f"volumes/{device_volume}/copy", {"name": device_copy_volume})
    failed_copy = wait_for(lambda: (j if (j := api(f"jobs/{failed_copy['job_id']}"))["status"] not in ("queued", "running") else None), 180)
    assert failed_copy["status"] == "failed" and "Removed the incomplete volume" in failed_copy["log"], failed_copy
    assert json.loads(docker("volume", "inspect", device_copy_volume, check=False) or b"[]") == []
    assert not docker("ps", "-aq", "--filter", "label=bilskirnir.volume-helper=true").strip()
    print("PASS a volume copies into a new one owned by the desktop user, and never over one", flush=True)
    # An MCP client moves archives with one-time links, without the dashboard session.
    exported_link = transfer_link({"operation": "volume_export", "volume": restored_volume})
    assert exported_link["method"] == "GET" and exported_link["url"].startswith("/api/transfers/"), exported_link
    with without_session(exported_link["url"]) as response:
        linked_export_job = response.headers["X-Job-Id"]
        linked_archive = response.read()
    assert job({"job_id": linked_export_job})["bytes"] == len(linked_archive)
    assert exported_link["url"].rsplit("/", 1)[1] not in api(f"jobs/{linked_export_job}")["log"]
    try:
        without_session(exported_link["url"]); raise AssertionError("a transfer link was used twice")
    except urllib.error.HTTPError as error:
        assert error.code == 404
    imported_link = transfer_link({"operation": "volume_import", "name": linked_volume})
    assert imported_link["method"] == "PUT", imported_link
    with without_session(imported_link["url"], data=linked_archive, method="PUT") as response:
        assert job(json.loads(response.read()))["name"] == linked_volume
    assert api(f"volumes/{linked_volume}/files/read", {"path": "/notes/renamed.txt"})["data"] == "Hello volume\n"
    with without_session(transfer_link({"operation": "volume_download", "volume": linked_volume, "path": "/notes/blob.bin"})["url"]) as response:
        assert response.read() == b"\x00\x01\x02"
    # A link runs its own operation only: an upload link is no download link.
    wrong_link = transfer_link({"operation": "volume_import", "name": f"bilskirnir-test-never-{tag}"})
    try:
        without_session(wrong_link["url"]); raise AssertionError("an import link downloaded")
    except urllib.error.HTTPError as error:
        assert error.code == 405
    assert json.loads(docker("volume", "inspect", f"bilskirnir-test-never-{tag}", check=False) or b"[]") == []
    print("PASS transfer links export, import and download a volume without a session", flush=True)
    try:
        api("volumes", {"name": share}); raise AssertionError("duplicate volume accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 409
    for target in ["//etc", "/./etc", "/var/run", "/home"]:
        try:
            api("containers", {"name":"invalid-mount","image":xfce["reference"],"mounts":[{"volume":share,"target":target}]})
            raise AssertionError(f"unsafe mount accepted: {target}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    # The manager's data volume holds its database and sessions, so no desktop mounts it. It
    # carries no Bilskirnir label, so the default VOLUME_FILTER leaves it out of the list too.
    marked = {item["Name"]: item["manager"] for item in api("volumes")}
    assert volume not in marked and marked[share] is False, marked
    assert json.loads(docker("volume", "inspect", share))[0]["Labels"].get("bilskirnir.managed") == "true"
    before = len(api("containers"))
    manager_mount = api("containers", {"name": "manager-volume", "image": xfce["reference"], "internet": False, "mounts": [{"volume": volume, "target": "/workspace", "readonly": True}]})
    failed = wait_for(lambda: (j if (j := api(f"jobs/{manager_mount['job_id']}"))["status"] not in ("queued", "running") else None))
    assert failed["status"] == "failed" and f"{volume} is the manager's own volume" in failed["log"], failed
    assert len(api("containers")) == before
    # A volume made outside Bilskirnir is out of reach: not listed, its files and copy answer as
    # for a missing volume, and no new container mounts it.
    docker("volume", "create", outside_volume)
    assert outside_volume not in {item["Name"] for item in api("volumes")}
    for path, body in [(f"volumes/{outside_volume}/files/list", {"path": "/"}), (f"volumes/{outside_volume}/copy", {"name": f"bilskirnir-test-never-{tag}"})]:
        try:
            api(path, body); raise AssertionError(f"{path} reached a filtered volume")
        except urllib.error.HTTPError as error:
            assert error.code == 404, (path, error.code)
    # Deleting it is refused the same way, and it stays.
    try:
        api(f"volumes/{outside_volume}", method="DELETE"); raise AssertionError("a filtered volume was deleted")
    except urllib.error.HTTPError as error:
        assert error.code == 404, error.code
    assert json.loads(docker("volume", "inspect", outside_volume))
    outside_mount = api("containers", {"name": "outside-volume", "image": xfce["reference"], "internet": False, "mounts": [{"volume": outside_volume, "target": "/workspace"}]})
    failed = wait_for(lambda: (j if (j := api(f"jobs/{outside_mount['job_id']}"))["status"] not in ("queued", "running") else None))
    assert failed["status"] == "failed" and "Volume not found" in failed["log"], failed
    assert len(api("containers")) == before
    assert json.loads(docker("volume", "inspect", f"bilskirnir-test-never-{tag}", check=False) or b"[]") == []
    print("PASS VOLUME_FILTER hides and refuses volumes made outside Bilskirnir", flush=True)
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/session", headers={"Host":"evil.example","Origin":"http://evil.example"})
    try:
        client.open(request); raise AssertionError("untrusted Host accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 403
    print("PASS duplicate volume, mount aliases and Host validation", flush=True)
    # The templates page reads the wiki's list in the browser, so the policy allows that host.
    assert api("settings")["templates_url"] == "https://raw.githubusercontent.com/wiki/co3moz/bilskirnir/templates/index.json"
    with client.open(f"http://127.0.0.1:{port}/", timeout=30) as response:
        policy = response.headers["Content-Security-Policy"]
    assert "connect-src 'self' https://raw.githubusercontent.com;" in policy, policy
    icon_png = png(64, 32)
    build_request = {"name": "offline-script-test", "description": "Built by the acceptance run", "base_image": xfce["reference"], "internet": False, "script": "set -eu\nprintf script-built > /opt/script-built\npython3 - <<'PY'\nimport socket\ns = socket.socket(); s.settimeout(2)\ntry:\n s.connect(('1.1.1.1',443))\nexcept OSError:\n pass\nelse:\n raise RuntimeError('offline build had internet')\nPY\n", "icon": "data:image/png;base64," + base64.b64encode(icon_png).decode(), "template_id": "acceptance"}
    for bad_field in [{"icon": "data:image/png;base64," + base64.b64encode(png(512, 512)).decode()}, {"template_id": "Not An Id"}]:
        try:
            api("images/build", {**build_request, **bad_field}); raise AssertionError(f"build accepted {list(bad_field)}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    build_job = api("images/build", build_request)
    built = job(build_job)
    # BuildKit's progress lines start each stage, and each one ends where the next begins.
    stages = api(f"jobs/{build_job['job_id']}")["stages"]
    assert [stage["name"] for stage in stages] == ["base", "script", "layer", "image", "cleanup"], stages
    assert all(stage["started_at"] <= stage["finished_at"] for stage in stages), stages
    assert all(a["finished_at"] == b["started_at"] for a, b in zip(stages, stages[1:])), stages
    # The exported configuration is the request again; lists and job results leave the script out.
    assert built["build"] == {"base_image": build_request["base_image"], "internet": False}, built
    assert api(f"images/{built['id']}/config") == {**build_request, "script": f"# acceptance run {tag}\n" + build_request["script"]}
    assert "script" not in next(image for image in api("images") if image["id"] == built["id"])["build"]
    # Lists name the icon by a version, and the icon route returns the picture itself.
    assert built["template_ids"] == ["acceptance"] and len(built["icon"]) == 16, built
    assert api(f"images/{built['id']}/icon?v={built['icon']}", raw=True) == icon_png
    print("PASS custom script image, offline build, persisted build log and exported configuration", flush=True)
    # A compose asks for values when a container is created and hands them to its setup script.
    compose = {"parameters": [{"name": "GREETING", "label": "Greeting", "required": True, "pattern": "[a-z]+", "pattern_message": "lowercase letters"}, {"name": "TOKEN", "required": True, "secret": True, "pattern": "\\S+"}, {"name": "FLAVOR", "type": "choice", "options": [{"value": "vanilla"}, {"value": "chocolate"}], "default": "vanilla"}], "script": "set -eu\nprintf '%s|%s|%s|%s' \"$GREETING\" \"$TOKEN\" \"$FLAVOR\" \"$(id -u)\" > ~/compose-setup\necho \"token is $TOKEN\"\n"}
    for invalid in [{**compose, "parameters": [{"name": "PATH"}]}, {**compose, "parameters": [{"name": "FLAVOR", "type": "choice"}]}, {**compose, "parameters": [{"name": "TOKEN", "secret": True, "default": "kept"}]}, {**compose, "parameters": [{"name": "GREETING", "pattern": "(?=a)a"}]}, {**compose, "parameters": [{"name": "GREETING", "type": "number", "pattern": "[0-9]+"}]}, {**compose, "parameters": [{"name": "GREETING", "pattern": "[a-z]+", "default": "Hello"}]}]:
        try:
            api(f"images/{built['id']}/compose", invalid, method="PUT"); raise AssertionError(f"invalid compose accepted: {invalid['parameters']}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    composed = api(f"images/{built['id']}/compose", compose, method="PUT")
    assert "script" not in composed["compose"] and [p["name"] for p in composed["compose"]["parameters"]] == ["GREETING", "TOKEN", "FLAVOR"]
    assert api(f"images/{built['id']}/compose")["script"] == compose["script"]
    assert api(f"images/{built['id']}/config")["compose"]["script"] == compose["script"]
    for values in [{}, {"GREETING": "hello", "TOKEN": "x", "UNKNOWN": "y"}, {"GREETING": "hello", "TOKEN": "x", "FLAVOR": "strawberry"}, {"GREETING": "Hello", "TOKEN": "x"}]:
        try:
            api("containers", {"name": "missing-values", "image": built["image_id"], "parameters": values}); raise AssertionError(f"compose values accepted: {values}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    # A secret that does not match its pattern is refused without the error repeating it.
    try:
        api("containers", {"name": "missing-values", "image": built["image_id"], "parameters": {"GREETING": "hello", "TOKEN": f"two words-{tag}"}}); raise AssertionError("a secret with a space was accepted")
    except urllib.error.HTTPError as error:
        body = error.read().decode()
        assert error.code == 400 and "expected format" in body and f"words-{tag}" not in body, body
    secret = f"s3cret-{tag}"
    spec = {"name": "GPU test", "image": built["image_id"], "resolution": "1280x720", "gpu": True, "gamepad": True, "internet": False, "audio": True, "microphone": True, "cpus": 2, "memory_mb": 2048, "mounts": [{"volume": share, "target": "/shared", "readonly": False}], "parameters": {"GREETING": "hello", "TOKEN": secret}}
    created = api("containers", spec)
    env_id = job(created)["container_id"]
    container_ids.append(env_id)
    container = f"bilskirnir-{env_id}"
    ready(container)
    # The job ended after the script ran as the desktop user; the secret reached only the script.
    assert docker("exec", container, "cat", "/home/desktop/compose-setup") == f"hello|{secret}|vanilla|1001".encode()
    created_log = api(f"jobs/{created['job_id']}")["log"]
    assert "token is ***" in created_log and secret not in created_log, created_log
    described = api(f"containers/{env_id}")
    assert secret not in json.dumps(described) and secret not in json.dumps(api("containers"))
    assert described["setup"] | {"finished_at": None} == {"status": "completed", "image": built["id"], "parameters": {"FLAVOR": "vanilla", "GREETING": "hello"}, "secrets": ["TOKEN"], "finished_at": None}, described
    print("PASS image compose asks for values, runs its setup as the desktop user and never returns a secret", flush=True)
    # Templates merged into one image: each part's script runs in a layer of its own in order,
    # and each compose step receives only its own values, although both ask for VERSION.
    merged_request = {"name": "merged-templates", "base_image": xfce["reference"], "internet": False, "template_ids": ["first", "second"],
        "parts": [{"id": "first", "name": "First", "script": "set -eu\nprintf first > /opt/merged-order\n"}, {"id": "second", "name": "Second", "script": "set -eu\nprintf ' second' >> /opt/merged-order\n"}],
        "compose": {"steps": [
            {"id": "first", "name": "First", "parameters": [{"name": "VERSION", "default": "1"}, {"name": "TOKEN", "secret": True}], "script": "set -eu\nprintf '%s|%s' \"$VERSION\" \"${OTHER-unset}\" > ~/first-step\nprintf '%s' \"$TOKEN\" > ~/first-token\n"},
            {"id": "second", "name": "Second", "parameters": [{"name": "VERSION", "default": "2"}, {"name": "OTHER"}], "script": "set -eu\nprintf '%s|%s' \"$VERSION\" \"${TOKEN-unset}\" > ~/second-step\necho \"saved $(cat ~/first-token)\"\n"}]}}
    for invalid in [{**merged_request, "script": "true"}, {**merged_request, "parts": [merged_request["parts"][0]] * 2}, {**merged_request, "compose": {"steps": [{**step, "id": "first"} for step in merged_request["compose"]["steps"]]}}]:
        assert refused("images/build", invalid) == 400, invalid
    merged_job = api("images/build", merged_request)
    merged_image = job(merged_job)
    assert merged_image["template_ids"] == ["first", "second"] and merged_image["build"]["parts"] == [{"id": "first", "name": "First"}, {"id": "second", "name": "Second"}], merged_image
    assert all("script" not in step for step in merged_image["compose"]["steps"]), merged_image["compose"]
    assert "Running the scripts of First, Second as root" in api(f"jobs/{merged_job['job_id']}")["log"]
    exported = api(f"images/{merged_image['id']}/config")
    assert exported["parts"][1] == merged_request["parts"][1] and "script" not in exported and exported["template_ids"] == ["first", "second"], exported
    merged_base = {"image": merged_image["image_id"], "internet": False}
    assert refused("containers", {**merged_base, "name": "merged-ambiguous", "parameters": {"VERSION": "3"}}) == 400
    merged_secret = f"merged-{tag}"
    created = api("containers", {**merged_base, "name": "Merged", "parameters": {"second.VERSION": "two", "TOKEN": merged_secret, "OTHER": "x"}})
    merged_id = job(created)["container_id"]
    container_ids.append(merged_id)
    ready(f"bilskirnir-{merged_id}")
    assert docker("exec", f"bilskirnir-{merged_id}", "cat", "/opt/merged-order") == b"first second"
    assert docker("exec", f"bilskirnir-{merged_id}", "cat", "/home/desktop/first-step") == b"1|unset"
    assert docker("exec", f"bilskirnir-{merged_id}", "cat", "/home/desktop/second-step") == b"two|unset"
    merged_log = api(f"jobs/{created['job_id']}")["log"]
    # The second step never gets the first one's secret, and its output masks what the first saved.
    assert "step 1 of 2, First" in merged_log and "step 2 of 2, Second" in merged_log and "saved ***" in merged_log and merged_secret not in merged_log, merged_log
    merged_setup = api(f"containers/{merged_id}")["setup"]
    assert merged_setup["parameters"] == {"first.VERSION": "1", "second.OTHER": "x", "second.VERSION": "two"} and merged_setup["secrets"] == ["first.TOKEN"], merged_setup
    assert [(step["id"], step["name"], step["status"]) for step in merged_setup["steps"]] == [("first", "First", "completed"), ("second", "Second", "completed")], merged_setup
    print("PASS templates merged into one image run their parts and setup steps in order, each step with its own values", flush=True)
    # Browser and Electron sandboxes start in a new user namespace. The desktop's seccomp profile
    # allows that and nothing else Docker reserves for CAP_SYS_ADMIN, such as mount(2).
    assert docker("exec", "--user", "desktop", container, "unshare", "--user", "--map-root-user", "true") == b""
    mounted = subprocess.run(["docker", "exec", "--user", "desktop", container, "unshare", "--user", "--map-root-user", "--mount", "--propagation", "unchanged", "mount", "-t", "tmpfs", "tmpfs", "/mnt"], capture_output=True)
    assert mounted.returncode != 0, mounted
    print("PASS user namespaces for browser sandboxes, mount still refused", flush=True)
    # Chinese, Japanese and Korean text shows as text, not boxes, also in screenshots agents read.
    for language in ("zh-cn", "ja", "ko"):
        assert b"Noto Sans CJK" in docker("exec", container, "fc-match", f"sans-serif:lang={language}"), language
    print("PASS CJK fonts for Chinese, Japanese and Korean text", flush=True)
    # GdkPixbuf draws SVG, so XFCE's default wallpaper and icons that exist only as SVG are not blank.
    loaders = docker("exec", container, "sh", "-c", "cat /usr/lib/*/gdk-pixbuf-2.0/2.10.0/loaders.cache")
    assert b'"image/svg+xml"' in loaders, loaders
    print("PASS SVG images load in the desktop", flush=True)
    default_exec = mcp_job(stdio_tool("exec_container", {"container_id":env_id,"script":"id -u\nprintf agent-install > /home/desktop/agent-marker"}))
    assert "1001" in default_exec["log"] and default_exec["result"]["user"] == "1001:1001"
    root_exec = mcp_job(tool("exec_container", {"container_id":env_id,"root":True,"script":"id -u\ninstall -m 644 /home/desktop/agent-marker /opt/agent-install"}))
    assert root_exec["result"]["user"] == "root"
    assert docker("exec",container,"cat","/opt/agent-install") == b"agent-install"
    failure = mcp_job(tool("exec_container", {"container_id":env_id,"script":"echo EXPECTED_EXEC_FAILURE\nexit 17"}), success=False)
    assert "EXPECTED_EXEC_FAILURE" in failure["log"] and "17" in failure["log"]
    timeout = mcp_job(tool("exec_container", {"container_id":env_id,"script":"sleep 5\nprintf must-not-exist > /home/desktop/after-timeout","timeout_seconds":1}), success=False)
    assert "124" in timeout["log"]
    time.sleep(1)
    assert docker("exec",container,"test","!","-e","/home/desktop/after-timeout") == b""
    written = stdio_tool("write_file", {"container_id":env_id,"path":"/home/desktop/agent-files/hello.txt","data":"Hello from MCP\n"})["structuredContent"]
    assert written["bytes"] == 15
    read = api(f"containers/{env_id}/files/read", {"path":written["path"]})
    assert read["data"] == "Hello from MCP\n" and read["eof"]
    assert docker("exec",container,"stat","-c","%u:%g %a",written["path"]).strip() == b"1001:1001 600"
    conflict = tool("write_file", {"container_id":env_id,"path":written["path"],"data":"no overwrite"}, allow_error=True)
    assert conflict["isError"]
    # Saving over a file keeps its permission bits, as the Files page's editor relies on.
    docker("exec","--user","desktop",container,"chmod","755",written["path"])
    api(f"containers/{env_id}/files/write", {"path":written["path"],"data":"Updated","replace":True})
    assert docker("exec",container,"stat","-c","%u:%g %a",written["path"]).strip() == b"1001:1001 755"
    page = tool("read_file", {"container_id":env_id,"path":written["path"],"offset":2,"limit":3})["structuredContent"]
    assert page["data"] == "dat" and page["next_offset"] == 5 and not page["eof"]
    binary = bytes(range(256))
    tool("write_file", {"container_id":env_id,"path":"/home/desktop/agent-files/agent.bin","encoding":"base64","data":base64.b64encode(binary).decode()})
    assert base64.b64decode(tool("read_file", {"container_id":env_id,"path":"/home/desktop/agent-files/agent.bin","encoding":"base64"})["structuredContent"]["data"]) == binary
    assert tool("read_file", {"container_id":env_id,"path":"/dev/zero"}, allow_error=True)["isError"]
    tool("upload_file", {"container_id":env_id,"destination":"/home/desktop/Downloads","name":"agent-upload.bin","data_base64":base64.b64encode(binary).decode()})
    assert docker("exec",container,"cat","/home/desktop/Downloads/agent-upload.bin") == binary
    image = stdio_tool("screenshot", {"container_id":env_id})["content"][0]
    assert image["type"] == "image" and image["mimeType"] == "image/jpeg" and base64.b64decode(image["data"]).startswith(b"\xff\xd8")
    saved = tool("save_screenshot", {"container_id":env_id,"path":"/home/desktop/Downloads/agent-shot.jpg","capture":{"max_width":320}})["structuredContent"]
    assert saved["path"] == "/home/desktop/Downloads/agent-shot.jpg" and saved["bytes"] > 0 and saved["width"] == 320
    assert saved["download_url"] == f"/api/containers/{env_id}/files/download?path=/home/desktop/Downloads/agent-shot.jpg"
    assert docker("exec",container,"cat",saved["path"]).startswith(b"\xff\xd8")
    assert tool("save_screenshot", {"container_id":env_id,"path":"/home/desktop/Downloads/agent-shot.jpg"}, allow_error=True)["isError"]
    assert tool("save_screenshot", {"container_id":env_id,"path":"/etc/agent-shot.jpg"}, allow_error=True)["isError"]
    assert api(f"containers/{env_id}/screenshots", {"path":"/home/desktop/Downloads/agent-shot.jpg","replace":True,"capture":{"max_width":160}})["width"] == 160
    request = urllib.request.Request(f"http://127.0.0.1:{port}{saved['download_url']}", headers={"X-Bilskirnir-Request":"1"})
    with client.open(request, timeout=60) as response:
        assert response.headers["Content-Type"] == "image/jpeg" and response.read().startswith(b"\xff\xd8")
    assert tool("read_file", {"container_id":env_id,"path":"/home/desktop/../etc/passwd"}, allow_error=True)["isError"]
    docker("exec", "--user", "desktop", container, "ln", "-s", "/home/desktop/agent-files", "/home/desktop/agent-link")
    assert tool("write_file", {"container_id":env_id,"path":"/home/desktop/agent-link/blocked.txt","data":"blocked"}, allow_error=True)["isError"]
    assert tool("read_file", {"container_id":env_id,"path":"/home/desktop/agent-link/hello.txt"}, allow_error=True)["isError"]
    print("PASS MCP/HTTP execution, exit failures, timeout, bounded file read/write, upload and screenshot", flush=True)
    docker("exec", "--detach", "--user", "desktop", "--env", "DISPLAY=:0", container, "bash", "-c", 'exec xev -geometry 320x180+80+80 -name "Bilskirnir input test" -event mouse -event keyboard > /home/desktop/input-events.log 2>&1')
    def input_position():
        info = docker("exec", "--env", "DISPLAY=:0",container,"xwininfo","-name","Bilskirnir input test").decode()
        import re
        x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info)[1]) + 160
        y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info)[1]) + 120
        return x,y
    x,y = wait_for(input_position)
    viewer = WebSocket(port, f"/api/containers/{env_id}/vnc", "; ".join(f"{c.name}={c.value}" for c in cookies))
    assert viewer.read(12).startswith(b"RFB ")
    viewer.send(b"RFB 003.008\n")
    types = viewer.read(viewer.read(1)[0]); assert 1 in types
    viewer.send(b"\x01"); assert viewer.read(4)==b"\0"*4
    viewer.send(b"\x01"); init=viewer.read(24); viewer.read(int.from_bytes(init[20:24],"big"))
    result = stdio_tool("desktop_input", {"container_id":env_id,"actions":[{"type":"move","x":x,"y":y},{"type":"click","x":x,"y":y},{"type":"type_text","text":"agent"},{"type":"key_press","keys":["Control","a"]},{"type":"scroll","x":x,"y":y,"direction":"up","steps":2}]})["structuredContent"]
    assert result["applied_actions"] == 5 and result["width"]==1280 and result["height"]==720
    api(f"containers/{env_id}/input", {"actions":[{"type":"drag","from_x":x,"from_y":y,"to_x":x+20,"to_y":y+10},{"type":"key_press","keys":["Escape"]}]})
    events = wait_for(lambda: (data if b"KeyPress" in (data:=docker("exec",container,"cat","/home/desktop/input-events.log")) and b"ButtonPress" in data and b"Control_L" in data else None))
    assert b"button 1" in events and b"button 4" in events and b"Escape" in events, events
    viewer.send(b"\x03\x00" + struct.pack("!HHHH",0,0,1,1))
    assert viewer.read(4)[0] == 0, "Agent input disconnected the existing VNC viewer"
    viewer.close()
    rejected = tool("desktop_input", {"container_id":env_id,"actions":[{"type":"click","x":2000,"y":y}]}, allow_error=True)
    assert rejected["isError"]
    print("PASS MCP/HTTP shared VNC mouse, keyboard, drag, scroll, Fence acknowledgement and bounds", flush=True)
    # The manager follows every running container once a second and keeps 30 minutes in memory.
    usage = wait_for(lambda: (u if (u := api("usage"))["available"] and any(c["id"] == env_id and len(c["samples"]) >= 3 for c in u["containers"]) else None), 30)
    mine = next(c for c in usage["containers"] if c["id"] == env_id)
    assert mine["cpu_limit"] == 2 and mine["memory_limit"] == 2048 * 1024 ** 2, mine
    last = mine["samples"][-1]
    assert set(last) == {"t", "cpu", "memory", "disk_read", "disk_write", "net_rx", "net_tx"}, last
    assert 0 <= last["cpu"] <= 2.5 and 0 < last["memory"] <= mine["memory_limit"], last
    stamps = [s["t"] for s in mine["samples"]]
    assert stamps == sorted(set(stamps)), stamps
    # The cursor returns only what came after it.
    def after_cursor():
        report = api(f"usage?since={usage['cursor']}")
        return report if any(c["id"] == env_id and c["samples"] for c in report["containers"]) else None
    newer = wait_for(after_cursor, 10)
    assert all(s["t"] > usage["cursor"] for c in newer["containers"] for s in c["samples"]), newer
    one = api(f"containers/{env_id}/usage")
    assert one["id"] == env_id and one["samples"] and "gpu" not in one, one
    # A GPU container runs, so the whole host GPU is sampled through a helper without a network.
    gpu = wait_for(lambda: (g if (g := api("usage")["gpu"])["state"] == "sampling" and g["devices"] and g["devices"][0]["samples"] else None), 90)
    assert gpu["scope"] == "host" and "NVIDIA" in gpu["devices"][0]["name"], gpu
    helpers = set(docker("ps", "-q", "--filter", "label=bilskirnir.usage-helper=true").decode().split()) - foreign_helpers
    assert len(helpers) == 1, helpers
    for helper in helpers:
        info = json.loads(docker("inspect", helper))[0]
        assert info["HostConfig"]["NetworkMode"] == "none" and info["HostConfig"]["ReadonlyRootfs"], info["HostConfig"]
        assert not any(m.get("Source", "").endswith("docker.sock") for m in info["Mounts"]), info["Mounts"]
    summary = tool("get_usage", {"container_id": env_id, "seconds": 30})["structuredContent"]
    assert summary["containers"][0]["id"] == env_id and summary["containers"][0]["cpu_cores"]["peak"] >= 0, summary
    assert summary["gpu"]["devices"] and summary["gpu"]["devices"][0]["utilization_percent"] is not None, summary
    print("PASS resource usage history, cursor, per-container route, host GPU helper and MCP summary", flush=True)
    # Script stdin is closed: a command reading input cannot consume the rest of the script.
    isolated = mcp_job(tool("exec_container", {"container_id":env_id,"script":"consumed=$(cat); echo \"stdin-bytes=${#consumed}\"\necho AFTER_STDIN\n"}))
    assert "stdin-bytes=0" in isolated["log"] and "AFTER_STDIN" in isolated["log"], isolated["log"]
    launched = tool("launch_application", {"container_id":env_id,"command":"bash","args":["-c","echo APP_LOG_LINE; exec xlogo -title 'Bilskirnir app test' -geometry 200x150+600+300"],"name":"xlogo test"})["structuredContent"]
    app_id = launched["application_id"]
    assert launched["state"] in ("running","starting"), launched
    running_app = wait_for(lambda: (s if (s := tool("get_application", {"container_id":env_id,"application_id":app_id})["structuredContent"])["state"] == "running" else None))
    assert running_app["pid"], running_app
    window = wait_for(lambda: next((w for w in tool("list_windows", {"container_id":env_id})["structuredContent"]["items"] if w["title"] == "Bilskirnir app test"), None))
    tool("focus_window", {"container_id":env_id,"window_id":window["window_id"]})
    geometry = tool("set_window_geometry", {"container_id":env_id,"window_id":window["window_id"],"x":100,"y":120,"width":320,"height":240})["structuredContent"]
    assert geometry["width"] == 320 and geometry["height"] == 240, geometry
    moved = wait_for(lambda: next((w for w in tool("list_windows", {"container_id":env_id})["structuredContent"]["items"] if w["window_id"] == window["window_id"] and w["width"] == 320), None))
    assert moved["active"], moved
    full = tool("screenshot", {"container_id":env_id})["structuredContent"]
    assert full["width"] == 1280 and full["height"] == 720, full
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/containers/{env_id}/screenshot?max_width=640", headers={"X-Bilskirnir-Request":"1"})
    with client.open(request, timeout=60) as response:
        assert response.headers["X-Image-Width"] == "640" and response.read().startswith(b"\xff\xd8")
    region = tool("screenshot", {"container_id":env_id,"region":"100,120,320,240","max_width":160})["structuredContent"]
    assert region["width"] == 160 and region["height"] == 120, region
    captured = tool("screenshot", {"container_id":env_id,"window_id":window["window_id"]})["structuredContent"]
    assert captured["width"] == 320 and captured["height"] == 240, captured
    observed = tool("act_and_observe", {"container_id":env_id,"actions":[{"type":"move","x":x,"y":y},{"type":"move_relative","dx":10,"dy":5},{"type":"hold_keys","keys":["w"],"milliseconds":300},{"type":"key_down","keys":["Shift"]}],"settle_ms":200,"capture":{"max_width":320}})
    assert observed["content"][0]["type"] == "image" and observed["structuredContent"]["width"] == 320, observed["structuredContent"]
    assert observed["structuredContent"]["input"]["released"]["keys"] == ["Shift"], observed["structuredContent"]
    assert observed["structuredContent"]["input"]["pointer"] == {"x":x+10,"y":y+5}, observed["structuredContent"]
    location = docker("exec","--user","desktop","--env","DISPLAY=:0",container,"xdotool","getmouselocation","--shell").decode()
    assert f"X={x+10}\n" in location and f"Y={y+5}\n" in location, location
    relative_only = api(f"containers/{env_id}/input", {"actions":[{"type":"move_relative","dx":-10,"dy":-5}]})
    assert relative_only["pointer"] == {"x":x,"y":y}, relative_only
    logs = tool("application_logs", {"container_id":env_id,"application_id":app_id,"tail_bytes":4096})["structuredContent"]
    assert "APP_LOG_LINE" in logs["log"], logs
    stopped = tool("stop_application", {"container_id":env_id,"application_id":app_id,"grace_seconds":3})["structuredContent"]
    assert stopped["state"] == "exited" and stopped["signal"] == 15, stopped
    assert any(a["application_id"] == app_id and a["state"] == "exited" for a in api(f"containers/{env_id}/applications")["items"])
    missing_program = tool("launch_application", {"container_id":env_id,"command":"/nonexistent/program"})["structuredContent"]
    assert missing_program["state"] == "failed", missing_program
    print("PASS application lifecycle, window management, sized screenshots, held keys and relative pointer", flush=True)
    second = tool("launch_application", {"container_id":env_id,"command":"bash","args":["-c","echo READY_MARKER; exec xlogo -title 'Bilskirnir wait test'"]})["structuredContent"]["application_id"]
    waited = tool("wait_for_condition", {"container_id":env_id,"window_title":"Bilskirnir wait test","application_id":second,"application_state":"running","log_contains":"READY_MARKER","timeout_ms":15000})["structuredContent"]
    assert waited["satisfied"] and waited["window"]["title"] == "Bilskirnir wait test" and waited["log_found"], waited
    stable = api(f"containers/{env_id}/wait", {"screen_stable_ms":600,"region":"900,500,200,120","timeout_ms":10000,"interval_ms":200})
    assert stable["satisfied"] and stable["screen_stable_for_ms"] >= 600, stable
    unmet = tool("wait_for_condition", {"container_id":env_id,"window_title":"no such window","timeout_ms":600,"interval_ms":200})["structuredContent"]
    assert not unmet["satisfied"] and unmet["window"] is None, unmet
    diagnostics = tool("application_diagnostics", {"container_id":env_id,"application_id":second})["structuredContent"]
    assert diagnostics["processes"] and "READY_MARKER" in diagnostics["log_tail"] and diagnostics["open_files"], diagnostics
    sampled = tool("sample_metrics", {"container_id":env_id,"application_id":second,"duration_ms":300})["structuredContent"]
    assert sampled["cpu_percent"] is not None and sampled["rss_bytes"] > 0 and sampled["processes"] >= 1, sampled
    recording = tool("start_recording", {"container_id":env_id,"fps":4,"max_seconds":20,"max_width":320,"name":"wait test"})["structuredContent"]
    time.sleep(1.5)
    finished = tool("stop_recording", {"container_id":env_id,"recording_id":recording["recording_id"]})["structuredContent"]
    assert finished["state"] == "finished" and finished["frames"] >= 2 and finished["reason"] == "stopped", finished
    assert any(r["recording_id"] == recording["recording_id"] and r["frames"] == finished["frames"] for r in tool("list_recordings", {"container_id":env_id})["structuredContent"]["items"])
    frames = tool("list_files", {"container_id":env_id,"path":finished["directory"]})["structuredContent"]
    assert sum(1 for f in frames["items"] if f["name"].endswith(".jpg")) == finished["frames"], frames
    exported = tool("export_artifacts", {"container_id":env_id,"destination":"/home/desktop/Downloads","name":"evidence.tar.gz","recording_ids":[recording["recording_id"]],"application_ids":[second]})["structuredContent"]
    listing = docker("exec", container, "tar", "-tzf", exported["path"]).decode()
    assert "manifest.json" in listing and f"recordings/{recording['recording_id']}/frame-00000" in listing and f"applications/{second}/log" in listing, listing
    assert tool("export_artifacts", {"container_id":env_id,"destination":"/home/desktop/Downloads","name":"evidence.tar.gz","recording_ids":[recording["recording_id"]]}, allow_error=True)["isError"]
    tool("move_file", {"container_id":env_id,"source":"/home/desktop/agent-files/hello.txt","destination":"/home/desktop/agent-files/renamed.txt"})
    entries = tool("list_files", {"container_id":env_id,"path":"/home/desktop/agent-files"})["structuredContent"]["items"]
    assert any(e["name"] == "renamed.txt" and e["type"] == "file" for e in entries) and not any(e["name"] == "hello.txt" for e in entries), entries
    assert tool("list_files", {"container_id":env_id,"path":"/home/desktop/agent-link"}, allow_error=True)["isError"]
    assert tool("delete_file", {"container_id":env_id,"path":"/home/desktop/agent-files"}, allow_error=True)["isError"]
    tool("delete_file", {"container_id":env_id,"path":"/home/desktop/agent-files","recursive":True})
    assert docker("exec",container,"test","!","-e","/home/desktop/agent-files") == b""
    tool("stop_application", {"container_id":env_id,"application_id":second})
    print("PASS waits, diagnostics, metrics, frame recordings, artifact export and file tools", flush=True)
    assert finished["video"]["codec"] == "MJPEG" and finished["video"]["bytes"] > 0, finished
    assert f"recordings/{recording['recording_id']}/recording.avi" in listing, listing
    downloaded = tool("download_file", {"container_id":env_id,"path":exported["path"]})["structuredContent"]
    assert downloaded["download_url"] == exported["download_url"] == f"/api/containers/{env_id}/files/download?path={exported['path']}"
    assert downloaded["bytes"] == exported["bytes"] and downloaded["mime_type"] == "application/gzip", downloaded["bytes"]
    assert base64.b64decode(downloaded["data_base64"])[:2] == b"\x1f\x8b"
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/containers/{env_id}/files/download?" + urllib.parse.urlencode({"path":exported["path"]}))
    with client.open(request, timeout=60) as response:
        assert response.headers["Content-Length"] == str(exported["bytes"]) and "evidence.tar.gz" in response.headers["Content-Disposition"], dict(response.headers)
        assert len(response.read()) == exported["bytes"]
    assert tool("download_file", {"container_id":env_id,"path":"/home/desktop/agent-link"}, allow_error=True)["isError"]
    # A folder downloads from the same route as a .tar.gz of itself, made as the desktop user while
    # it streams and recorded as a job; symlinks inside stay links, one on the path is refused.
    docker("exec", "--user", "desktop", container, "sh", "-c", "mkdir -p '/home/desktop/agent folder/sub' && printf folder > '/home/desktop/agent folder/sub/a.txt' && ln -sfn /etc '/home/desktop/agent folder/etc-link' && ln -sfn '/home/desktop/agent folder' /home/desktop/folder-link")
    def folder_download(url, opener=client):
        with opener.open(url, timeout=130) as response:
            return response.headers, response.read()
    headers, folder_data = folder_download(f"http://127.0.0.1:{port}/api/containers/{env_id}/files/download?" + urllib.parse.urlencode({"path":"/home/desktop/agent folder"}))
    assert headers["Content-Type"] == "application/gzip" and "filename*=UTF-8''agent%20folder.tar.gz" in headers["Content-Disposition"], dict(headers)
    assert job({"job_id": headers["X-Job-Id"]}) == {"path": "/home/desktop/agent folder", "bytes": len(folder_data)}
    with tarfile.open(fileobj=io.BytesIO(folder_data), mode="r:gz") as archive:
        assert archive.extractfile("agent folder/sub/a.txt").read() == b"folder"
        assert archive.getmember("agent folder/etc-link").issym(), archive.getnames()
        assert not any(name.startswith("agent folder/etc-link/") for name in archive.getnames()), archive.getnames()
        assert {member.uid for member in archive.getmembers()} == {1001}
    for path in ["/home/desktop/folder-link", "/home/desktop/folder-link/sub"]:
        try:
            folder_download(f"http://127.0.0.1:{port}/api/containers/{env_id}/files/download?" + urllib.parse.urlencode({"path":path})); raise AssertionError(f"{path} was downloaded through a symlink")
        except urllib.error.HTTPError as error:
            assert error.code == 400, (path, error.code)
    # A transfer link downloads a folder the same way, without a session.
    link = transfer_link({"operation":"container_download","container_id":env_id,"path":"/home/desktop/agent folder/sub"})
    headers, linked = folder_download(f"http://127.0.0.1:{port}{link['url']}", urllib.request.build_opener())
    with tarfile.open(fileobj=io.BytesIO(linked), mode="r:gz") as archive:
        assert archive.extractfile("sub/a.txt").read() == b"folder"
    assert job({"job_id": headers["X-Job-Id"]})["bytes"] == len(linked)
    # A download that stops early leaves no tar running in the desktop.
    docker("exec", "--user", "desktop", container, "sh", "-c", "head -c 67108864 /dev/urandom > '/home/desktop/agent folder/noise'")
    with client.open(f"http://127.0.0.1:{port}/api/containers/{env_id}/files/download?" + urllib.parse.urlencode({"path":"/home/desktop/agent folder"}), timeout=130) as response:
        cancelled = response.headers["X-Job-Id"]
        response.read(65536)
    stopped = wait_for(lambda: (j if (j := api(f"jobs/{cancelled}"))["status"] not in ("queued", "running") else None), 60)
    assert stopped["status"] == "failed", stopped
    wait_for(lambda: docker("exec", container, "pgrep", "-x", "tar|gzip", check=False).strip() == b"" or None, 30)
    docker("exec", "--user", "desktop", container, "rm", "-rf", "/home/desktop/agent folder", "/home/desktop/folder-link")
    print("PASS folder download as .tar.gz, through a transfer link, and its cancellation", flush=True)
    tone =b"".join(struct.pack("<ff", 0.25 * math.sin(i * 2 * math.pi * 440 / 48000), 0.25 * math.sin(i * 2 * math.pi * 440 / 48000)) for i in range(240000))
    player = subprocess.Popen(["docker", "exec", "-i", "--user", "desktop", container, "pacat", "--playback", "--device=desktop-output", "--format=float32le", "--rate=48000", "--channels=2"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    threading.Thread(target=lambda: (player.stdin.write(tone), player.stdin.close()), daemon=True).start()
    time.sleep(2.0)
    heard = tool("capture_audio", {"container_id":env_id,"seconds":1.0})
    assert heard["content"][0]["type"] == "audio" and heard["content"][0]["mimeType"] == "audio/wav", heard["content"][0].keys()
    assert heard["structuredContent"]["peak"] > 0.01 and not heard["structuredContent"]["silent"], heard["structuredContent"]
    assert base64.b64decode(heard["content"][0]["data"])[:4] == b"RIFF"
    player.wait(timeout=30)
    pcm = b"".join(struct.pack("<h", int(8000 * math.sin(i * 2 * math.pi * 440 / 48000))) for i in range(240000))
    wav = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, 48000, 96000, 2, 16) + b"data" + struct.pack("<I", len(pcm)) + pcm
    injector = threading.Thread(target=lambda: tool("inject_audio", {"container_id":env_id,"data_base64":base64.b64encode(wav).decode()}), daemon=True)
    injector.start()
    time.sleep(2.0)
    spoken = tool("capture_audio", {"container_id":env_id,"seconds":1.0,"source":"microphone"})["structuredContent"]
    assert spoken["peak"] > 0.01, spoken
    injector.join(timeout=30)
    print("PASS file download, MJPEG video, audio capture and microphone injection", flush=True)
    term = tool("open_terminal", {"container_id":env_id})["structuredContent"]
    assert term["state"] in ("running", "starting"), term
    tool("terminal_input", {"container_id":env_id,"terminal_id":term["terminal_id"],"text":"echo TERMINAL_$((20+22)); pwd"})
    shown = wait_for(lambda: (o if "TERMINAL_42" in (o := tool("terminal_output", {"container_id":env_id,"terminal_id":term["terminal_id"]})["structuredContent"])["output"] and "/home/desktop" in o["output"] else None))
    assert shown["state"] == "running" and shown["next_offset"] > 0, shown
    later = tool("terminal_output", {"container_id":env_id,"terminal_id":term["terminal_id"],"offset":shown["next_offset"]})["structuredContent"]
    assert "TERMINAL_42" not in later["output"], later
    root_term = tool("open_terminal", {"container_id":env_id,"root":True,"cwd":"/opt"})["structuredContent"]
    tool("terminal_input", {"container_id":env_id,"terminal_id":root_term["terminal_id"],"text":"id -u; pwd"})
    wait_for(lambda: "\n0\n" in tool("terminal_output", {"container_id":env_id,"terminal_id":root_term["terminal_id"]})["structuredContent"]["output"] and "/opt" in tool("terminal_output", {"container_id":env_id,"terminal_id":root_term["terminal_id"]})["structuredContent"]["output"])
    for handle in (term, root_term):
        closed = tool("close_terminal", {"container_id":env_id,"terminal_id":handle["terminal_id"]})["structuredContent"]
        assert closed["state"] == "exited", closed
    assert len(tool("list_terminals", {"container_id":env_id})["structuredContent"]["items"]) == 2
    print("PASS interactive terminals as the desktop user and root", flush=True)
    gtk = tool("launch_application", {"container_id":env_id,"command":"xfce4-terminal","args":["--title","Bilskirnir a11y test","--disable-server"],"env":{"GTK_MODULES":"atk-bridge"}})["structuredContent"]
    assert wait_for(lambda: tool("wait_for_condition", {"container_id":env_id,"window_title":"Bilskirnir a11y test","timeout_ms":20000})["structuredContent"]["satisfied"] or None)
    gtk_pid = tool("get_application", {"container_id":env_id,"application_id":gtk["application_id"]})["structuredContent"]["pid"]
    tree = wait_for(lambda: (t if (t := tool("inspect_ui", {"container_id":env_id,"pid":gtk_pid,"max_depth":8})["structuredContent"])["applications"] else None), 30)
    assert tree["nodes"] > 1 and tree["applications"][0]["role"] == "application", tree
    def roles(node):
        yield node["role"]
        for child in node.get("children", []):
            yield from roles(child)
    assert "frame" in set(roles(tree["applications"][0])), set(roles(tree["applications"][0]))
    tool("stop_application", {"container_id":env_id,"application_id":gtk["application_id"]})
    print("PASS accessibility tree inspection of a GTK application", flush=True)
    pad = tool("gamepad_input", {"container_id":env_id,"actions":[{"type":"reset"}]})["structuredContent"]
    assert pad["state"] == "running" and pad["device"].startswith("/dev/input/event"), pad
    reader = subprocess.Popen(["docker", "exec", "--user", "desktop", container, "python3", "-c", "import struct,sys,time\nf=open(sys.argv[1],'rb',buffering=0)\nseen=[]\nwhile len(seen)<3:\n    data=f.read(24)\n    if not data: break\n    _,_,t,c,v=struct.unpack('<qqHHi',data)\n    if t in (1,3): seen.append((t,c,v))\nprint(seen)", pad["device"]], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    time.sleep(0.5)
    pad = tool("gamepad_input", {"container_id":env_id,"actions":[{"type":"button","button":"a"},{"type":"axis","axis":"left_x","value":0.5},{"type":"wait","milliseconds":100},{"type":"button","button":"a","pressed":False}]})["structuredContent"]
    assert pad["applied_actions"] == 4 and pad["buttons"].get("a") is False and abs(pad["axes"]["left_x"] - 0.5) < 1e-6, pad
    events = reader.communicate(timeout=20)[0].decode()
    assert "(1, 304, 1)" in events and "(3, 0, 16384)" in events and "(1, 304, 0)" in events, events
    assert tool("gamepad_input", {"container_id":clone_id if False else env_id,"actions":[{"type":"button","button":"select"}]}, allow_error=True)["isError"]
    print("PASS virtual gamepad device, button and axis events", flush=True)
    payload = bytes(range(256)) * 5000
    uploaded = upload(env_id,"binary & notes.bin",payload)
    assert docker("exec",container,"cat",uploaded["path"]) == payload
    assert docker("exec",container,"stat","-c","%u:%g",uploaded["path"]).strip() == b"1001:1001"
    empty = upload(env_id,"empty.txt",b"")
    assert docker("exec",container,"cat",empty["path"]) == b""
    try:
        upload(env_id,"binary & notes.bin",b"must-not-replace")
        raise AssertionError("existing file was overwritten without consent")
    except urllib.error.HTTPError as error:
        assert error.code == 409
    assert docker("exec",container,"cat",uploaded["path"]) == payload
    upload(env_id,"binary & notes.bin",b"replaced",replace=True)
    assert docker("exec",container,"cat",uploaded["path"]) == b"replaced"
    pushed = transfer_link({"operation":"container_upload","container_id":env_id,"destination":"/home/desktop/Downloads","name":"linked.bin"})
    with without_session(pushed["url"],data=payload,method="PUT") as response:
        assert json.loads(response.read())["bytes"] == len(payload)
    assert docker("exec",container,"cat","/home/desktop/Downloads/linked.bin") == payload
    with without_session(transfer_link({"operation":"container_download","container_id":env_id,"path":"/home/desktop/Downloads/linked.bin"})["url"]) as response:
        assert response.read() == payload
    try:
        without_session(pushed["url"],data=b"again",method="PUT"); raise AssertionError("an upload link was used twice")
    except urllib.error.HTTPError as error:
        assert error.code == 404
    print("PASS transfer links upload to and download from a container without a session", flush=True)
    # A program's own HTTP service in the desktop, through the manager, for the MCP token only.
    listed = mcp_job(tool("update_container",{"container_id":env_id,"ports":[8765]}))
    assert listed["result"]["live"] == ["ports"] and listed["result"]["restart"] == [], listed
    served = tool("launch_application",{"container_id":env_id,"command":"python3","args":["-m","http.server","8765","--bind","0.0.0.0","--directory","/home/desktop/Downloads"],"name":"proxy test"})["structuredContent"]["application_id"]
    proxied = f"http://127.0.0.1:{port}/api/containers/{env_id}/ports/8765/linked.bin"
    def through_proxy(url=proxied):
        request = urllib.request.Request(url,headers={"Authorization":f"Bearer {mcp_token}"})
        with urllib.request.build_opener().open(request,timeout=30) as response:
            return response.read(), response.headers.get_all("Content-Security-Policy")
    proxied_body, policies = wait_for(through_proxy, 60)
    assert proxied_body == payload and "sandbox" in policies, policies
    try:
        client.open(proxied,timeout=30); raise AssertionError("the dashboard session opened a proxied port")
    except urllib.error.HTTPError as error:
        assert error.code == 401
    try:
        through_proxy(f"http://127.0.0.1:{port}/api/containers/{env_id}/ports/8766/"); raise AssertionError("an unlisted port was proxied")
    except urllib.error.HTTPError as error:
        assert error.code == 403
    tool("stop_application",{"container_id":env_id,"application_id":served})
    mcp_job(tool("update_container",{"container_id":env_id,"ports":[]}))
    print("PASS a listed port of a running desktop is proxied for the MCP token, not the session", flush=True)
    # MCP servers inside the desktop: a stdio program registered through the administrator's tool,
    # an HTTP server on the desktop's loopback registered with bilskirnir-mcp, both reached through
    # a scoped endpoint whose token opens nothing else.
    tool("write_file",{"container_id":env_id,"path":"/home/desktop/mcp/echo.py","data":ECHO_SERVER})
    tool("write_file",{"container_id":env_id,"path":"/home/desktop/mcp/echo_http.py","data":ECHO_HTTP_SERVER})
    http_server = tool("launch_application",{"container_id":env_id,"command":"python3","args":["/home/desktop/mcp/echo_http.py"],"name":"mcp http test"})["structuredContent"]["application_id"]
    registered = tool("register_mcp_server",{"container_id":env_id,"name":"echo","command":"python3","args":["/home/desktop/mcp/echo.py"]})["structuredContent"]
    assert registered["state"] == "ready" and [t["name"] for t in registered["tools"]] == ["echo"], registered
    docker("exec","--user","1001:1001",container,"bilskirnir-mcp","add","api","--url","http://127.0.0.1:8931/mcp","--header","X-Test: yes")
    def server_ready(name):
        servers = tool("list_mcp_servers",{"container_id":env_id})["structuredContent"]["items"]
        found = next((s for s in servers if s["name"] == name), None)
        return found if found and found["state"] == "ready" else None
    api_server = wait_for(lambda: server_ready("api"), 60)
    assert api_server["source"] == "container" and api_server["headers"] == ["X-Test"] and "yes" not in json.dumps(api_server), api_server
    endpoints = tool("list_mcp_endpoints")["structuredContent"]["items"]
    own = [e for e in endpoints if e["automatic"] and e["grants"][0]["container_id"] == env_id]
    assert own and own[0]["has_token"] is False, endpoints
    created = tool("create_mcp_endpoint",{"name":"acceptance-mcp","grants":[{"container_id":env_id,"scopes":["view"],"servers":"all"}]})["structuredContent"]
    scoped = created["token"]
    session = endpoint_session("acceptance-mcp", scoped)
    offered = {t["name"] for t in endpoint_call("acceptance-mcp", scoped, session, "tools/list")["tools"]}
    assert {"screenshot","echo__echo","api__echo"} <= offered and not offered & {"exec_container","desktop_input","delete_container","create_container"}, offered
    echoed = endpoint_call("acceptance-mcp", scoped, session, "tools/call", {"name":"echo__echo","arguments":{"text":"hi"}})
    assert echoed["content"][0]["text"] == "echo: hi uid=1001 display=:0", echoed
    echoed = endpoint_call("acceptance-mcp", scoped, session, "tools/call", {"name":"api__echo","arguments":{"text":"over http"}})
    assert echoed["content"][0]["text"] == "echo: over http header=yes", echoed
    shot = endpoint_call("acceptance-mcp", scoped, session, "tools/call", {"name":"screenshot","arguments":{"max_width":320}})
    assert shot["content"][0]["type"] == "image" and shot["structuredContent"]["width"] == 320, shot
    # Single tools are left out by the names the endpoint gives them, a container tool's and a
    # server's alike; the list above, without one, was everything the scope and the servers give.
    assert registered["tools"][0]["exposed"] == "echo__echo", registered
    narrowed = tool("update_mcp_endpoint",{"name":"acceptance-mcp","grants":[{"container_id":env_id,"scopes":["view"],"servers":"all","excluded_tools":["save_screenshot","echo__echo"]}]})["structuredContent"]
    assert narrowed["grants"][0]["excluded_tools"] == ["save_screenshot","echo__echo"], narrowed
    offered = {t["name"] for t in endpoint_call("acceptance-mcp", scoped, session, "tools/list")["tools"]}
    assert {"screenshot","api__echo"} <= offered and not offered & {"save_screenshot","echo__echo"}, offered
    for name in ["save_screenshot","echo__echo"]:
        _, reply = endpoint_post("acceptance-mcp", scoped, {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":{"text":"hi"}}}, session)
        assert "Unknown tool" in reply["error"]["message"], reply
    assert tool("update_mcp_endpoint",{"name":"acceptance-mcp","grants":[{"container_id":env_id,"scopes":["view"],"servers":"all","excluded_tools":["desktop_input"]}]}, allow_error=True)["isError"]
    print("PASS an endpoint leaves single container and server tools out by name", flush=True)
    try:
        mcp("tools/list", token=scoped); raise AssertionError("an endpoint token opened the administrator's channel")
    except urllib.error.HTTPError as error:
        assert error.code == 401
    tool("remove_mcp_server",{"container_id":env_id,"name":"echo"})
    offered = {t["name"] for t in endpoint_call("acceptance-mcp", scoped, session, "tools/list")["tools"]}
    assert "echo__echo" not in offered and "api__echo" in offered, offered
    assert tool("remove_mcp_server",{"container_id":env_id,"name":"api"})["structuredContent"]["removed"]
    tool("delete_mcp_endpoint",{"name":"acceptance-mcp"})
    # Whether an endpoint exists is told only to a token that opens something: the revoked token
    # gets 401, the administrator's 404.
    try:
        endpoint_session("acceptance-mcp", scoped); raise AssertionError("a deleted endpoint answered")
    except urllib.error.HTTPError as error:
        assert error.code == 401, error.code
    try:
        endpoint_session("acceptance-mcp", mcp_token); raise AssertionError("a deleted endpoint answered the administrator")
    except urllib.error.HTTPError as error:
        assert error.code == 404, error.code
    tool("stop_application",{"container_id":env_id,"application_id":http_server})
    print("PASS desktop MCP servers are reached through a scoped endpoint as the desktop user", flush=True)
    docker("exec",container,"chown","1001:1001","/shared")
    volume_upload = upload(env_id,"project.txt",b"persistent-upload",destination="/shared/uploads")
    assert docker("exec",container,"cat",volume_upload["path"]) == b"persistent-upload"
    # An archive unpacks into a new folder of the volume this running desktop mounts, which sees
    # it at once, owned by the desktop user. An existing folder, the volume's root while it holds
    # files, a missing volume and the manager's own volume are refused; a failed extract leaves
    # no folder.
    def extract(name, path, data):
        query = urllib.parse.urlencode({"path": path, "size": len(data)})
        request = urllib.request.Request(f"http://127.0.0.1:{port}/api/volumes/{name}/extract?{query}", data=data, method="POST", headers={"Content-Type": "application/octet-stream", "X-Bilskirnir-Request": "1"})
        with client.open(request, timeout=130) as response:
            return json.loads(response.read())
    project = io.BytesIO()
    with tarfile.open(fileobj=project, mode="w:gz") as archive:
        for name, content in {"src/main.cs": b"class Main {}\n", "README.md": b"worktree\n"}.items():
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(content), 0o644
            archive.addfile(info, io.BytesIO(content))
    project = project.getvalue()
    extracted = job(extract(share, "/worktree", project))
    assert extracted == {"volume": share, "path": "/worktree", "bytes": len(project)}, extracted
    assert docker("exec", container, "cat", "/shared/worktree/src/main.cs") == b"class Main {}\n"
    assert docker("exec", container, "stat", "-c", "%U", "/shared/worktree", "/shared/worktree/src", "/shared/worktree/README.md") == b"desktop\ndesktop\ndesktop\n"
    missing_volume = f"bilskirnir-test-missing-{tag}"
    for name, path, status in [(share, "/worktree", 409), (share, "/", 409), (share, "/absent/worktree", 400), (missing_volume, "/worktree", 404), (volume, "/worktree", 403)]:
        try:
            extract(name, path, project); raise AssertionError(f"extract into {name}:{path} accepted")
        except urllib.error.HTTPError as error:
            assert error.code == status, (name, path, error.code)
    assert json.loads(docker("volume", "inspect", missing_volume, check=False) or b"[]") == []
    assert docker("exec", container, "cat", "/shared/worktree/README.md") == b"worktree\n"
    # An MCP client unpacks through a one-time link, without a session, streaming without a size.
    linked = transfer_link({"operation": "volume_extract", "volume": share, "path": "/linked-worktree"})
    assert linked["method"] == "PUT", linked
    with without_session(linked["url"], data=iter([project[:1000], project[1000:]]), method="PUT") as response:
        assert job(json.loads(response.read()))["path"] == "/linked-worktree"
    assert docker("exec", container, "cat", "/shared/linked-worktree/src/main.cs") == b"class Main {}\n"
    try:
        started = extract(share, "/broken", project[: len(project) // 2])
    except urllib.error.HTTPError as error:
        assert error.code == 400, error.code
    else:
        finished = wait_for(lambda: (j if (j := api(f"jobs/{started['job_id']}"))["status"] not in ("queued", "running") else None), 180)
        assert finished["status"] == "failed", finished
    assert subprocess.run(["docker", "exec", container, "test", "-e", "/shared/broken"]).returncode != 0
    assert not docker("ps", "-aq", "--filter", "label=bilskirnir.volume-helper=true").strip()
    print("PASS archives unpack into a new folder of a mounted volume as the desktop user, also through a link, never over one", flush=True)
    # Anywhere in the home will do, and the upload creates the folders it needs; a folder can also
    # be made first, as the dashboard's folder browser does.
    picked = upload(env_id, "picked.txt", b"picked", destination="/home/desktop/Documents/picked")
    assert picked["path"] == "/home/desktop/Documents/picked/picked.txt", picked
    assert docker("exec", container, "stat", "-c", "%U", "/home/desktop/Documents/picked") == b"desktop\n"
    assert api(f"containers/{env_id}/files/mkdir", {"path": "/home/desktop/Documents/made"}) == {"path": "/home/desktop/Documents/made", "created": True}
    assert refused(f"containers/{env_id}/files/mkdir", {"path": "/home/desktop/Documents/made"}) == 409
    assert refused(f"containers/{env_id}/files/mkdir", {"path": "/etc/made"}) == 400
    docker("exec",container,"ln","-s","/tmp","/home/desktop/Downloads/escape")
    for target in ["/etc", "/home/other", "/home/desktop/Downloads/../.ssh", "/home/desktop/Downloads/escape"]:
        try:
            upload(env_id,"forbidden.txt",b"blocked",destination=target)
            raise AssertionError(f"unsafe upload accepted: {target}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    assert docker("exec",container,"test","!","-e","/tmp/forbidden.txt") == b""
    assert docker("exec",manager,"ls","-A","/data/uploads") == b""
    # Logout must revoke a transfer that is still receiving a body.
    query = urllib.parse.urlencode({"destination":"/home/desktop/Downloads","name":"interrupted.bin","size":10*1024*1024})
    transfer = http.client.HTTPConnection("127.0.0.1",port,timeout=10)
    transfer.putrequest("POST",f"/api/containers/{env_id}/files?{query}")
    transfer.putheader("Content-Type","application/octet-stream")
    transfer.putheader("Content-Length",10*1024*1024)
    transfer.putheader("X-Bilskirnir-Request","1")
    transfer.putheader("Cookie","; ".join(f"{c.name}={c.value}" for c in cookies))
    transfer.endheaders()
    transfer.send(b"x" * 128000)
    wait_for(lambda: docker("exec",manager,"ls","-A","/data/uploads"))
    api("logout",{})
    assert transfer.getresponse().status == 401
    transfer.close()
    wait_for(lambda: docker("exec",manager,"ls","-A","/data/uploads") == b"")
    assert docker("exec",container,"test","!","-e","/home/desktop/Downloads/interrupted.bin") == b""
    login()
    print("PASS binary/empty uploads, any home folder, new folders, desktop ownership, volume persistence, overwrite protection, symlinks and revoked transfer cleanup",flush=True)
    assert docker("exec", container, "cat", "/opt/script-built") == b"script-built"
    diag = api(f"containers/{env_id}/diagnostics")["output"]
    assert "D3D12 (NVIDIA" in diag, diag
    assert "Default Source: browser-microphone" in diag, diag
    # A program laid out like a Chromium build turns VirtualGL's Chrome workaround on for itself,
    # also when the panel starts it; other programs, and those a browser starts, keep the default.
    chromium = "\n".join([
        r'd=$(mktemp -d -p "$HOME") && cp /usr/bin/env "$d/chrome" && touch "$d/icudtl.dat" "$d/resources.pak"',
        r'echo "browser=$(gpu-run "$d/chrome" | grep ^VGL_CHROMEHACK=)"',
        r'echo "explicit=$(VGL_CHROMEHACK=0 gpu-run "$d/chrome" | grep ^VGL_CHROMEHACK=)"',
        r'echo "game=$(gpu-run /usr/bin/env | grep -c ^VGL_CHROMEHACK=)"',
        r'echo "started-by-browser=$(gpu-run "$d/chrome" /usr/bin/env | grep -c ^VGL_CHROMEHACK=)"',
        r'echo "panel=$(tr "\0" "\n" < /proc/$(pgrep -x xfce4-panel)/environ | grep -c "^LD_PRELOAD=.*/chromium-gpu.so")"',
        r'rm -rf "$d"', ""])
    hack = mcp_job(tool("exec_container", {"container_id":env_id,"script":chromium}))["log"]
    for line in ["browser=VGL_CHROMEHACK=1", "explicit=VGL_CHROMEHACK=0", "game=0", "started-by-browser=0", "panel=1"]:
        assert line in hack.splitlines(), hack
    # A maximized OpenGL window is rendered at its new size, also for a program that takes its
    # events the way Godot's editor does; at the old size its menu bar and top rows went missing.
    resize = mcp_job(tool("exec_container", {"container_id":env_id,"script":f"gpu-run python3 - <<'PY'\n{GL_RESIZE_PROBE}PY\n"}))["log"]
    gl_window, gl_drawable = (part.split("=")[1] for part in next(line for line in resize.splitlines() if line.startswith("window=")).split())
    assert gl_window != "400x300" and gl_drawable == gl_window, resize
    print("PASS independent desktop, hardware OpenGL, resized OpenGL windows and virtual audio devices", flush=True)
    jpeg = api(f"containers/{env_id}/screenshot", raw=True)
    assert jpeg.startswith(b"\xff\xd8") and len(jpeg) > 1000
    cookie = "; ".join(f"{c.name}={c.value}" for c in cookies)
    ws = WebSocket(port, f"/api/containers/{env_id}/vnc", cookie)
    assert ws.read(12).startswith(b"RFB 003.")
    ws.send(b"RFB 003.008\n")
    n = ws.read(1)[0]
    assert 1 in ws.read(n)
    ws.send(b"\x01"); assert ws.read(4) == b"\0\0\0\0"
    ws.send(b"\x01")
    server_init = ws.read(24)
    assert struct.unpack("!HH", server_init[:4]) == (1280, 720)
    ws.read(struct.unpack("!I", server_init[20:24])[0]); ws.close()
    try:
        WebSocket(port, f"/api/containers/{env_id}/vnc", cookie, "https://evil.example")
        raise AssertionError("cross-origin VNC accepted")
    except RuntimeError as error:
        assert "403" in str(error)
    print("PASS JPEG preview, authenticated VNC handshake and origin restriction", flush=True)
    output = WebSocket(port, f"/api/containers/{env_id}/audio/output", cookie)
    tone = b"".join(struct.pack("<ff", 0.25 * math.sin(i * 2 * math.pi * 440 / 48000), 0.25 * math.sin(i * 2 * math.pi * 440 / 48000)) for i in range(24000))
    player = subprocess.Popen(["docker", "exec", "-i", "--user", "desktop", container, "pacat", "--playback", "--device=desktop-output", "--format=float32le", "--rate=48000", "--channels=2"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    player.stdin.write(tone); player.stdin.close()
    found = False
    for _ in range(40):
        samples = struct.unpack("<" + "f" * 1024, output.read(4096))
        if max(map(abs, samples)) > 0.01:
            found = True; break
    output.close(); player.wait(timeout=10)
    assert found, "Audio channel did not carry desktop sound"
    mic = WebSocket(port, f"/api/containers/{env_id}/audio/input", cookie)
    mic.send(struct.pack("<" + "f" * 4096, *([0.1] * 4096)))
    live_vnc = WebSocket(port, f"/api/containers/{env_id}/vnc", cookie)
    assert live_vnc.read(12).startswith(b"RFB 003.")
    live_output = WebSocket(port, f"/api/containers/{env_id}/audio/output", cookie)
    live_output.read(4096)
    api("logout", {})
    for connection in [live_vnc, live_output, mic]:
        connection.assert_closed(); connection.close()
    cookies.clear(); login()
    # Recorders must also terminate inside the container, not only in the manager.
    wait_for(lambda: subprocess.run(["docker","exec",container,"pgrep","-x","parec"],capture_output=True).returncode != 0, 10)
    print("PASS logout closes VNC/audio/microphone and container-side recorders", flush=True)
    print("PASS desktop audio streaming and microphone input channel", flush=True)
    offline_script = "import socket; s=socket.socket(); s.settimeout(2); s.connect(('1.1.1.1',443))"
    assert subprocess.run(["docker", "exec", container, "python3", "-c", offline_script], capture_output=True).returncode != 0
    job(api(f"containers/{env_id}/internet", {"enabled": True}))
    assert subprocess.run(["docker", "exec", container, "python3", "-c", offline_script], capture_output=True).returncode == 0
    job(api(f"containers/{env_id}/internet", {"enabled": False}))
    assert api(f"containers/{env_id}/screenshot", raw=True).startswith(b"\xff\xd8")
    assert subprocess.run(["docker", "exec", container, "python3", "-c", offline_script], capture_output=True).returncode != 0
    print("PASS live internet on/off with VNC still reachable", flush=True)
    docker("exec", "--user", "desktop", container, "bash", "-c", "printf snapshot-home > /home/desktop/snapshot-marker; printf volume-data > /shared/volume-marker")
    snap_job = api(f"containers/{env_id}/snapshot", {"name": "snapshot-test"})
    snap = job(snap_job)
    # Each stage of the snapshot ran and ended where the next began; the commit has no progress.
    snap_record = api(f"jobs/{snap_job['job_id']}")
    snap_stages = snap_record["stages"]
    assert [stage["name"] for stage in snap_stages if "started_at" in stage] == ["check", "commit", "record"], snap_stages
    assert all(a["finished_at"] == b["started_at"] for a, b in zip(snap_stages, snap_stages[1:])), snap_stages
    assert "progress" not in snap_record and "Stage 2/3: Committing the container layer\n" in snap_record["log"], snap_record
    assert snap["desktop"]["catalog_id"] == "ubuntu-xfce"
    assert snap["parent_image_id"] == built["image_id"] and "build" not in snap
    # A snapshot keeps the icon of the image its container came from, but no template.
    assert snap["icon"] == built["icon"] and "template_ids" not in snap, snap
    try:
        api(f"images/{snap['id']}/config"); raise AssertionError("snapshot exported a build configuration")
    except urllib.error.HTTPError as error:
        assert error.code == 404
    base_layers = json.loads(docker("image", "inspect", built["image_id"]))[0]["RootFS"]["Layers"]
    snap_layers = json.loads(docker("image", "inspect", snap["image_id"]))[0]["RootFS"]["Layers"]
    assert snap_layers[:len(base_layers)] == base_layers and len(snap_layers) > len(base_layers)
    # The snapshot keeps the compose its container ran, so a container from it asks again.
    assert [p["name"] for p in snap["compose"]["parameters"]] == ["GREETING", "TOKEN", "FLAVOR"] and "script" not in snap["compose"]
    spec.update({"name": "Clone", "image": snap["image_id"], "gpu": False, "gamepad": False, "audio": False, "microphone": False, "mounts": [{"volume": clone_share, "target": "/shared"}], "parameters": {"GREETING": "again", "TOKEN": "other", "FLAVOR": ""}})
    clone_id = job(api("containers", spec))["container_id"]
    container_ids.append(clone_id)
    clone = f"bilskirnir-{clone_id}"
    ready(clone)
    assert docker("exec", clone, "cat", "/home/desktop/snapshot-marker") == b"snapshot-home"
    assert docker("exec", clone, "cat", "/home/desktop/compose-setup") == b"again|other||1001"
    assert subprocess.run(["docker", "exec", clone, "test", "-e", "/shared/volume-marker"]).returncode != 0
    print("PASS layered snapshot, immutable ancestry and independent volume clone", flush=True)
    # The snapshot leaves as a gzip-compressed docker save stream and comes back under a new name.
    archive_dir = tempfile.mkdtemp(prefix=f"bilskirnir-archive-{tag}-")
    exported = os.path.join(archive_dir, "snapshot.tar.gz")
    with client.open(urllib.request.Request(f"http://127.0.0.1:{port}/api/images/{snap['id']}/archive"), timeout=1800) as response, open(exported, "wb") as out:
        assert response.headers["Content-Type"] == "application/gzip"
        assert 'filename="snapshot-test.tar.gz"' in response.headers["Content-Disposition"]
        export_job = response.headers["X-Job-Id"]
        shutil.copyfileobj(response, out, 1 << 20)
    assert job({"job_id": export_job})["bytes"] == os.path.getsize(exported)
    with tarfile.open(exported, "r|gz") as saved:
        first = saved.next()
        assert first.name == "bilskirnir.json", first.name
        metadata = json.load(saved.extractfile(first))
        assert (metadata["name"], metadata["kind"], metadata["image_id"]) == ("snapshot-test", "snapshot", snap["image_id"]), metadata
        manifest = next(json.load(saved.extractfile(member)) for member in saved if member.name == "manifest.json")
        assert len(manifest) == 1 and not manifest[0].get("RepoTags"), manifest
    imported = job(import_archive(exported))
    assert (imported["kind"], imported["name"], imported["image_id"]) == ("archive", "snapshot-test", snap["image_id"]), imported
    assert imported["reference"] == f"bilskirnir/imported:{imported['id']}" and imported["desktop"] == snap["desktop"], imported
    assert imported["parent_image_id"] == snap["parent_image_id"], imported
    assert json.loads(docker("image", "inspect", imported["reference"]))[0]["Id"] == snap["image_id"]
    renamed = job(import_archive(exported, name="Renamed archive", description="From acceptance"))
    assert (renamed["name"], renamed["description"]) == ("Renamed archive", "From acceptance"), renamed
    # A script build keeps its configuration, and its result leaves the script out as a build's does.
    built_archive = os.path.join(archive_dir, "built.tar.gz")
    with client.open(urllib.request.Request(f"http://127.0.0.1:{port}/api/images/{built['id']}/archive"), timeout=1800) as response, open(built_archive, "wb") as out:
        built_export = response.headers["X-Job-Id"]
        shutil.copyfileobj(response, out, 1 << 20)
    job({"job_id": built_export})
    moved = job(import_archive(built_archive))
    assert moved["build"] == built["build"] and moved["image_id"] == built["image_id"], moved
    assert moved["compose"] == composed["compose"], moved
    assert api(f"images/{moved['id']}/config") == api(f"images/{built['id']}/config")
    # Names inside an archive never reach Docker: an existing tag keeps pointing at its image. The
    # target is this run's own copy of the catalog tag, so a regression cannot break the catalog image.
    guard = f"bilskirnir-archive-guard:{tag}"
    docker("tag", xfce["reference"], guard)
    image_refs.append(guard)
    guard_id = json.loads(docker("image", "inspect", guard))[0]["Id"]
    hostile = os.path.join(archive_dir, "hostile.tar.gz")
    synthetic_archive(hostile, {"bilskirnir.desktop": "true", "bilskirnir.managed": "true", "bilskirnir.test": tag}, [guard, f"bilskirnir-archive-victim:{tag}"])
    loaded = job(import_archive(hostile, name="Hostile names"))
    assert json.loads(docker("image", "inspect", guard))[0]["Id"] == guard_id
    assert json.loads(docker("image", "inspect", f"bilskirnir-archive-victim:{tag}", check=False) or b"[]") == []
    tagged = json.loads(docker("image", "inspect", loaded["reference"]))[0]
    assert tagged["RepoTags"] == [loaded["reference"]] and tagged["Config"]["Labels"]["bilskirnir.test"] == tag, tagged["RepoTags"]
    # An archive without a desktop image is refused before Docker may register anything.
    refused_archive = os.path.join(archive_dir, "refused.tar.gz")
    synthetic_archive(refused_archive, {"bilskirnir.test-refused": tag}, [f"bilskirnir-archive-refused:{tag}"])
    try:
        import_archive(refused_archive)
        raise AssertionError("an archive without a desktop image was accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 400 and "desktop image" in json.loads(error.read())["error"]
    assert not docker("image", "ls", "-q", "--filter", f"label=bilskirnir.test-refused={tag}").strip()
    shutil.rmtree(archive_dir)
    print("PASS snapshot archive export/import keeps metadata and never applies names from an archive", flush=True)
    # An image goes by itself only while no container starts from it. The archives of the snapshot
    # name the clone's source too, so each goes with its own tag while the image stays; then the
    # snapshot itself is refused. An image nothing else holds goes from Docker with its record.
    def image_refused(record_id):
        try:
            api(f"images/{record_id}", method="DELETE")
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read() or b"{}").get("error", "")
        raise AssertionError(f"DELETE images/{record_id} was accepted")
    assert api(f"images/{renamed['id']}", method="DELETE") == {"id": renamed["id"], "name": "Renamed archive", "deleted": True, "image_removed": False}
    assert json.loads(docker("image", "inspect", renamed["reference"], check=False) or b"[]") == []
    untagged = tool("delete_image", {"image_id": imported["id"]})["structuredContent"]
    assert untagged == {"id": imported["id"], "name": "snapshot-test", "deleted": True, "image_removed": False}, untagged
    assert json.loads(docker("image", "inspect", snap["reference"]))[0]["Id"] == snap["image_id"]
    status, message = image_refused(snap["id"])
    assert status == 409 and message.startswith('"Clone" was created from this image'), (status, message)
    refused_tool = tool("delete_image", {"image_id": snap["id"]}, allow_error=True)
    assert refused_tool["isError"] and '"Clone"' in refused_tool["content"][0]["text"], refused_tool
    assert image_refused("0123456789abcdef0123456789abcdef")[0] == 404
    hostile_id = json.loads(docker("image", "inspect", loaded["reference"]))[0]["Id"]
    assert api(f"images/{loaded['id']}", method="DELETE") == {"id": loaded["id"], "name": "Hostile names", "deleted": True, "image_removed": True}
    assert json.loads(docker("image", "inspect", hostile_id, check=False) or b"[]") == []
    listed_images = {image["id"] for image in api("images")}
    assert snap["id"] in listed_images and not listed_images & {renamed["id"], imported["id"], loaded["id"]}, listed_images
    print("PASS an image is deleted only while no container needs it, and Docker keeps one another tag holds", flush=True)
    # BuildKit holds a line without its newline until the step ends, so the marker needs one to
    # reach the job log while the script still sleeps.
    slow = api("images/build", {"name":"concurrent-build","base_image":xfce["reference"],"internet":False,"script":"set -eu\necho SLOW_STARTED\nsleep 20\necho SLOW_FINISHED\n"})
    wait_for(lambda: "SLOW_STARTED" in api(f"jobs/{slow['job_id']}")["log"])
    started = time.monotonic()
    job(api(f"containers/{clone_id}/action", {"action": "stop"}))
    assert time.monotonic() - started < 8, "stop blocked by unrelated build"
    assert api(f"jobs/{slow['job_id']}")["status"] == "running"
    job(slow)
    print("PASS long image build does not block other container mutations",flush=True)
    job(api(f"containers/{clone_id}/action", {"action": "start"}))
    ready(clone)
    assert docker("exec", clone, "cat", "/home/desktop/snapshot-marker") == b"snapshot-home"
    # Delete preflight must leave the writable layer intact when a network has a foreign endpoint.
    docker("run","-d","--name",foreign,"--label",f"bilskirnir.test={tag}","--network",f"bilskirnir-{clone_id}-egress","--entrypoint","sleep",xfce["reference"],"300")
    deletion = api(f"containers/{clone_id}", method="DELETE")
    failed = wait_for(lambda: (j if (j := api(f"jobs/{deletion['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"] == "failed",failed
    assert docker("exec",clone,"cat","/home/desktop/snapshot-marker") == b"snapshot-home"
    docker("rm","--force",foreign)
    print("PASS delete preflight preserves container when network has another endpoint",flush=True)
    # Recover a manually removed container using its existing networks and pinned source image.
    docker("rm","--force",clone)
    docker("volume","rm",clone_share)
    missing_volume=api(f"containers/{clone_id}/retry",{})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{missing_volume['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "volume" in failed["log"].lower(),failed
    assert json.loads(docker("volume","inspect",clone_share,check=False))==[]
    api("volumes",{"name":clone_share})
    job(api(f"containers/{clone_id}/retry",{}));ready(clone)
    assert docker("exec",clone,"cat","/home/desktop/snapshot-marker") == b"snapshot-home"
    # The recreated container starts from the source image, so the compose setup ran again.
    assert docker("exec",clone,"cat","/home/desktop/compose-setup") == b"again|other||1001"
    assert api(f"containers/{clone_id}")["setup"]["status"] == "completed"
    print("PASS missing container retry reuses container identity and immutable image",flush=True)
    # One malformed container must not prevent other desktops from reconnecting after restart.
    corrupt_network=f"bilskirnir-{clone_id}-egress"
    docker("network","rm",corrupt_network)
    docker("network","create","--label",f"bilskirnir.test={tag}",corrupt_network)
    records = {record["id"] for record in api("containers")}
    assert {env_id, clone_id, merged_id} <= records, records
    docker("rm", "--force", manager)
    start_manager(); cookies.clear(); login()
    wait_for(lambda: api(f"containers/{env_id}/screenshot", raw=True).startswith(b"\xff\xd8"), 50)
    assert {record["id"] for record in api("containers")} == records, records
    docker("network","rm",corrupt_network)
    job(api(f"containers/{clone_id}/retry",{}))
    print("PASS malformed container network does not block reconciliation of others",flush=True)
    print("PASS stop/start persistence and manager replacement/network reconciliation", flush=True)
    gnome_spec = {"name":"GNOME GPU test","image":gnome["reference"],"gpu":True,"internet":False,"memory_mb":3072,"cpus":2,"audio":True,"microphone":True,"resolution":"1280x720"}
    gnome_id=job(api("containers",gnome_spec))["container_id"]
    container_ids.append(gnome_id)
    gnome_container=f"bilskirnir-{gnome_id}"
    ready(gnome_container)
    assert b"GNOME Shell 46" in docker("exec",gnome_container,"gnome-shell","--version")
    assert subprocess.run(["docker","exec",gnome_container,"pgrep","-x","gnome-shell"],capture_output=True).returncode==0
    assert subprocess.run(["docker","exec",gnome_container,"pgrep","-f","^/usr/libexec/gnome-session-failed"],capture_output=True).returncode!=0
    # A browser GNOME Shell starts gets the Chromium check too.
    shell_env = docker("exec","--user","desktop",gnome_container,"sh","-c","tr '\\0' '\\n' < /proc/$(pgrep -x gnome-shell)/environ")
    assert any(line.startswith(b"LD_PRELOAD=") and b"/chromium-gpu.so" in line for line in shell_env.splitlines()), shell_env
    assert api(f"containers/{gnome_id}/screenshot",raw=True).startswith(b"\xff\xd8")
    assert "D3D12 (NVIDIA" in api(f"containers/{gnome_id}/diagnostics")["output"]
    gnome_snapshot=job(api(f"containers/{gnome_id}/snapshot",{"name":"GNOME snapshot"}))
    assert gnome_snapshot["desktop"]["session"] == "gnome"
    assert gnome_snapshot["parent_image_id"] == gnome["image_id"]
    gnome_spec.update({"name":"GNOME CPU clone","image":gnome_snapshot["image_id"],"gpu":False,"audio":False,"microphone":False})
    gnome_clone_id=job(api("containers",gnome_spec))["container_id"]
    container_ids.append(gnome_clone_id)
    ready(f"bilskirnir-{gnome_clone_id}")
    assert subprocess.run(["docker","exec",f"bilskirnir-{gnome_clone_id}","pgrep","-x","gnome-shell"],capture_output=True).returncode==0
    assert subprocess.run(["docker","exec",f"bilskirnir-{gnome_clone_id}","pgrep","-f","^/usr/libexec/gnome-session-failed"],capture_output=True).returncode!=0
    docker("exec","-d","--user","desktop",f"bilskirnir-{gnome_clone_id}","bash","-c","exec -a /usr/libexec/gnome-session-failed sleep 30")
    assert subprocess.run(["docker","exec",f"bilskirnir-{gnome_clone_id}","/usr/local/bin/desktop-healthcheck"],capture_output=True).returncode!=0
    print("PASS GNOME Shell GPU desktop, preview, snapshot metadata and CPU clone",flush=True)
    # An OCI start failure leaves a discoverable container/intent that retry can repair in place.
    broken_reference=f"bilskirnir/test-broken:{tag}"
    fixture_container=f"bilskirnir-test-image-{tag}"
    docker("create","--name",fixture_container,"--label",f"bilskirnir.test={tag}",xfce["reference"])
    docker("commit","--change",'ENTRYPOINT ["/usr/local/bin/desktop-missing-entrypoint"]',fixture_container,broken_reference)
    image_refs.append(broken_reference)
    volume_reference=f"bilskirnir/test-volume:{tag}"
    docker("commit","--change",'VOLUME ["/home/desktop"]',fixture_container,volume_reference)
    image_refs.append(volume_reference);docker("rm",fixture_container)
    before=len(api("containers"))
    implicit_volume=api("containers",{"name":"implicit-volume","image":volume_reference,"internet":False})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{implicit_volume['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "VOLUME" in failed["log"],failed
    assert len(api("containers"))==before
    print("PASS implicit image VOLUME cannot silently exclude home from snapshots",flush=True)
    broken={"image_id":json.loads(docker("image","inspect",broken_reference))[0]["Id"]}
    create_failed = api("containers",{"name":"repairable-start","image":broken["image_id"],"internet":False,"gpu":False})
    failed = wait_for(lambda: (j if (j := api(f"jobs/{create_failed['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"] == "failed",failed
    repair = next(x for x in api("containers") if x["spec"]["name"]=="repairable-start")
    repair_id=repair["id"];container_ids.append(repair_id);repair_container=repair["container"]
    assert repair["last_error"]
    previous_id=json.loads(docker("inspect",repair_container))[0]["Id"]
    with tempfile.TemporaryDirectory() as directory:
        restored=os.path.join(directory,"desktop-entrypoint")
        with open(restored,"wb") as file:
            file.write(docker("run","--rm","--entrypoint","cat",xfce["reference"],"/usr/local/bin/desktop-entrypoint"))
        os.chmod(restored,0o755)
        docker("cp",restored,f"{repair_container}:/usr/local/bin/desktop-missing-entrypoint")
    job(api(f"containers/{repair_id}/retry",{}));ready(repair_container)
    assert json.loads(docker("inspect",repair_container))[0]["Id"]==previous_id
    assert not next(x for x in api("containers") if x["id"]==repair_id)["last_error"]
    print("PASS failed setup retry repairs existing container without discarding its layer",flush=True)
    # Binary / very long script output cannot abort or exhaust manager log buffers.
    noisy=api("images/build",{"name":"binary-long-log","base_image":xfce["reference"],"internet":False,"script":"python3 -c \"import sys; sys.stdout.buffer.write(bytes([255])+b'x'*300000)\"\n"})
    job(noisy)
    assert len(api(f"jobs/{noisy['job_id']}")["log"].encode())<=262144
    assert docker("exec",manager,"test","!","-e",f"/data/builds/{noisy['job_id']}")==b""
    assert not docker("image","ls","-q",f"bilskirnir/build-base:{noisy['job_id']}")
    print("PASS bounded binary build logs and temporary script/tag cleanup",flush=True)
    bad=api("images/build",{"name":"failed-script","base_image":xfce["reference"],"internet":False,"script":"echo EXPECTED_SCRIPT_FAILURE\nexit 17\n"})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{bad['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "EXPECTED_SCRIPT_FAILURE" in failed["log"],failed
    # The script's stage is the last one started, so the panel shows where the build stopped.
    assert [s["name"] for s in failed["stages"] if "started_at" in s]==["base","script"],failed["stages"]
    assert "Stage 2/5: Running the setup script\n" in failed["log"],failed["log"]
    assert docker("exec",manager,"test","!","-e",f"/data/builds/{bad['job_id']}")==b""
    assert not docker("image","ls","-q",f"bilskirnir/build-base:{bad['job_id']}")
    print("PASS failed build also removes temporary scripts and pinned base tag",flush=True)
    agent_image = mcp_job(tool("build_image", {"name":"agent-build","base_image":xfce["reference"],"internet":False,"script":f"# acceptance {tag}\nprintf mcp-built > /opt/mcp-built"}))["result"]
    agent_env = mcp_job(tool("create_container", {"name":"Agent lifecycle","image":agent_image["image_id"],"internet":False,"gpu":False}))["result"]["container_id"]
    container_ids.append(agent_env)
    ready(f"bilskirnir-{agent_env}")
    assert tool("get_container", {"container_id":agent_env})["structuredContent"]["state"] == "running"
    # The container's machine identity is its record's, from the files the image carried too.
    identity = tool("get_container", {"container_id":agent_env})["structuredContent"]["spec"]
    assert identity["hostname"] == f"bilskirnir-{agent_env}", identity
    assert machine(f"bilskirnir-{agent_env}") == recorded(identity), (machine(f"bilskirnir-{agent_env}"), identity)
    assert tool("read_file", {"container_id":agent_env,"path":"/opt/mcp-built"})["structuredContent"]["data"] == "mcp-built"
    tool("write_file", {"container_id":agent_env,"path":"/home/desktop/from-agent.txt","data":"snapshot from MCP"})
    agent_snapshot = mcp_job(tool("snapshot", {"container_id":agent_env,"name":"Agent snapshot"}))["result"]
    assert agent_snapshot["parent_image_id"] == agent_image["image_id"]
    assert any(image["id"] == agent_snapshot["id"] for image in tool("list_images")["structuredContent"]["items"])
    reset_job = mcp_job(tool("reset_container", {"container_id":agent_env}))
    assert reset_job["result"]["volumes_preserved"] and "Recreated the container from its saved source image" in reset_job["log"], reset_job
    ready(f"bilskirnir-{agent_env}")
    assert tool("read_file", {"container_id":agent_env,"path":"/home/desktop/from-agent.txt"}, allow_error=True)["isError"]
    assert tool("read_file", {"container_id":agent_env,"path":"/opt/mcp-built"})["structuredContent"]["data"] == "mcp-built"
    print("PASS container reset discards the container layer and keeps the pinned image", flush=True)
    assert tool("get_container", {"container_id":agent_env})["structuredContent"]["spec"] == identity
    assert machine(f"bilskirnir-{agent_env}") == recorded(identity)
    mcp_job(tool("set_internet", {"container_id":agent_env,"enabled":True}))
    # The internet interface gets the same address where the Engine API uses one given on connect.
    engine_api = tuple(int(part) for part in docker("version", "--format", "{{.Server.APIVersion}}").decode().split("."))
    online = machine(f"bilskirnir-{agent_env}")
    if engine_api >= (1, 54):
        assert online == recorded(identity, interfaces=2), (online, identity)
    else:
        assert identity["mac_address"] in online["macs"] and len(online["macs"]) == 2, online
    mcp_job(tool("set_internet", {"container_id":agent_env,"enabled":False}))
    mcp_job(tool("container_action", {"container_id":agent_env,"action":"stop"}))
    mcp_job(tool("container_action", {"container_id":agent_env,"action":"start"}))
    ready(f"bilskirnir-{agent_env}")
    assert machine(f"bilskirnir-{agent_env}") == recorded(identity)
    # A retry that has to create the container again keeps the identity as well.
    docker("rm", "--force", f"bilskirnir-{agent_env}")
    mcp_job(tool("retry_container", {"container_id":agent_env}))
    ready(f"bilskirnir-{agent_env}")
    assert machine(f"bilskirnir-{agent_env}") == recorded(identity)
    print("PASS machine identity survives reset, internet changes, restart and a recreating retry", flush=True)
    deleted = mcp_job(tool("delete_container", {"container_id":agent_env}))
    assert deleted["result"]["volumes_preserved"] and deleted["result"]["images_preserved"]
    assert "Named volumes and saved images were preserved" in deleted["log"]
    container_ids.remove(agent_env)
    clone = mcp_job(tool("create_container", {"name":"Agent snapshot clone","image":agent_snapshot["image_id"],"gpu":False,"internet":False}))["result"]["container_id"]
    container_ids.append(clone)
    ready(f"bilskirnir-{clone}")
    assert tool("read_file", {"container_id":clone,"path":"/home/desktop/from-agent.txt"})["structuredContent"]["data"] == "snapshot from MCP"
    # The snapshot carries the source's machine ID; the clone is another machine all the same.
    clone_identity = tool("get_container", {"container_id":clone})["structuredContent"]["spec"]
    assert all(clone_identity[key] != identity[key] for key in ("hostname", "mac_address", "machine_id")), clone_identity
    assert machine(f"bilskirnir-{clone}") == recorded(clone_identity)
    mcp_job(tool("delete_container", {"container_id":clone}))
    container_ids.remove(clone)
    print("PASS complete MCP image build, desktop create, snapshot, clone, internet, lifecycle and deletion", flush=True)
    # Agents read and change composes; the list shows the parameters without the script.
    assert tool("get_image_compose", {"image_id": built["id"]})["structuredContent"]["script"] == compose["script"]
    listed = next(image for image in tool("list_images")["structuredContent"]["items"] if image["id"] == built["id"])
    assert listed["compose"]["parameters"] == composed["compose"]["parameters"] and "script" not in listed["compose"]
    tool("set_image_compose", {"image_id": moved["id"], "compose": {"script": "true"}})
    assert api(f"images/{moved['id']}/compose") == {"parameters": [], "script": "true", "root": False, "timeout_seconds": 600}
    assert "compose" not in tool("set_image_compose", {"image_id": moved["id"], "compose": None})["structuredContent"]
    print("PASS MCP compose read, replace and removal", flush=True)
    # A volume parameter mounts the volume chosen for it at its path whenever the container is
    # created; without one the path stays in the container. Either way the desktop user owns the
    # folder before the setup script runs, which reads the path and the volume's name.
    volume_compose = {"parameters": [{"name": "UNITY_HOME", "type": "volume", "path": "/unity-install-location", "required": True}, {"name": "UNITY_CACHE", "type": "volume", "path": "/home/desktop/Unity/Cache"}, {"name": "UNITY_SETTINGS", "type": "volume", "path": "/home/desktop/.config/unity3d"}, {"name": "UNITY_PROJECT", "type": "folder", "label": "Unity project", "default": "/home/desktop/UnityProjects/MyGame"}], "script": "set -eu\nprintf '%s|%s|%s|%s|%s' \"$UNITY_HOME\" \"$UNITY_HOME_VOLUME\" \"$UNITY_CACHE\" \"$UNITY_CACHE_VOLUME\" \"$(id -u)\" > \"$UNITY_HOME/unity_v1\"\nprintf cached > \"$UNITY_CACHE/marker\"\nmkdir -p \"$UNITY_PROJECT\"\n"}
    for parameters in [[{"name": "UNITY_HOME", "type": "volume"}], [{"name": "UNITY_HOME", "type": "volume", "path": "/tmp/unity"}], [{"name": "UNITY_HOME", "type": "volume", "path": "/opt/../etc"}], [*volume_compose["parameters"], {"name": "UNITY_HOME_VOLUME"}], [{"name": "UNITY_PROJECT", "type": "folder", "default": "/etc/projects"}]]:
        assert refused(f"images/{built['id']}/compose", {**volume_compose, "parameters": parameters}, "PUT") == 400, parameters
    installs_image = job(api("images/build", {"name": "volume-parameters", "base_image": xfce["reference"], "internet": False, "script": f"# volume parameters {tag}\ntrue\n", "compose": volume_compose}))
    assert installs_image["compose"]["parameters"] == [{"label": "", "description": "", "required": False, "secret": False, "default": "", **p} for p in volume_compose["parameters"]], installs_image["compose"]
    api("volumes", {"name": installs_volume})
    base = {"image": installs_image["image_id"], "internet": False}
    # Refused before a job: no volume for a required parameter, a volume also in the volume
    # settings or chosen twice, and another volume at the parameter's path.
    for name, parameters, mounts in [("no-volume", {}, []), ("twice", {"UNITY_HOME": installs_volume}, [{"volume": installs_volume, "target": "/srv/unity"}]), ("same-path", {"UNITY_HOME": installs_volume}, [{"volume": share, "target": "/unity-install-location"}]), ("same-volume", {"UNITY_HOME": installs_volume, "UNITY_CACHE": installs_volume}, [])]:
        assert refused("containers", {**base, "name": name, "parameters": parameters, "mounts": mounts}) == 400, name
    # Refused by the job: a volume that does not exist is not created, and the manager's own data
    # volume is never handed to a desktop.
    for name, chosen, reason in [("absent", absent_volume, "Volume not found"), ("manager", volume, "manager's own volume")]:
        response = api("containers", {**base, "name": name, "parameters": {"UNITY_HOME": chosen}})
        failed = wait_for(lambda: (j if (j := api(f"jobs/{response['job_id']}"))["status"] not in ("queued", "running") else None))
        assert failed["status"] == "failed" and reason in failed["log"], failed
    assert json.loads(docker("volume", "inspect", absent_volume, check=False) or b"[]") == []
    assert not [item for item in api("containers") if item["spec"]["name"] in ("absent", "manager")]
    def volume_mounts(container):
        return {m["Destination"]: m["Name"] for m in json.loads(docker("inspect", container))[0]["Mounts"] if m["Type"] == "volume"}
    kept = job(api("containers", {**base, "name": "Volume parameter", "parameters": {"UNITY_HOME": installs_volume}}))["container_id"]
    container_ids.append(kept)
    kept_container = f"bilskirnir-{kept}"
    ready(kept_container)
    assert docker("exec", kept_container, "cat", "/unity-install-location/unity_v1") == f"/unity-install-location|{installs_volume}|/home/desktop/Unity/Cache||1001".encode()
    assert volume_mounts(kept_container) == {"/unity-install-location": installs_volume}
    assert docker("exec", kept_container, "stat", "-c", "%U", "/unity-install-location", "/home/desktop/Unity", "/home/desktop/Unity/Cache") == b"desktop\ndesktop\ndesktop\n"
    assert api(f"containers/{kept}")["setup"]["mounts"] == [{"volume": installs_volume, "target": "/unity-install-location", "readonly": False}]
    # The folders the compose names take uploads, a volume parameter's outside the home too, and
    # the record lists them for the upload dialog.
    assert api(f"containers/{kept}")["setup"]["places"] == [{"label": "UNITY_HOME", "path": "/unity-install-location"}, {"label": "UNITY_CACHE", "path": "/home/desktop/Unity/Cache"}, {"label": "UNITY_SETTINGS", "path": "/home/desktop/.config/unity3d"}, {"label": "Unity project", "path": "/home/desktop/UnityProjects/MyGame"}]
    assert docker("exec", kept_container, "stat", "-c", "%U", "/home/desktop/UnityProjects/MyGame") == b"desktop\n"
    assert upload(kept, "asset.txt", b"asset", destination="/unity-install-location/Assets")["path"] == "/unity-install-location/Assets/asset.txt"
    assert docker("exec", kept_container, "stat", "-c", "%U", "/unity-install-location/Assets/asset.txt") == b"desktop\n"
    print("PASS a compose's volume and folder parameters are listed as places that take uploads", flush=True)
    try:
        upload(kept, "a.txt", b"a", destination="/unity-install-locationish")
        raise AssertionError("an upload beside a compose folder was accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 400, error.code
    # The volume settings cannot put another volume at its path, and a recreation keeps it.
    assert refused(f"containers/{kept}/settings", {"mounts": [{"volume": share, "target": "/unity-install-location"}]}) == 400
    job(api(f"containers/{kept}/settings", {"resolution": "1280x720"}))
    image_refs.append(api(f"containers/{kept}")["working_image"]["reference"])
    ready(kept_container)
    assert volume_mounts(kept_container) == {"/unity-install-location": installs_volume}
    # A reset starts from the image again: the volume keeps its files, the folder without a volume
    # loses them, and the setup runs again.
    docker("exec", "--user", "desktop", kept_container, "sh", "-c", "printf kept > /unity-install-location/extra && printf layer > /home/desktop/Unity/Cache/extra")
    job(api(f"containers/{kept}/reset", {}))
    ready(kept_container)
    assert volume_mounts(kept_container) == {"/unity-install-location": installs_volume}
    assert docker("exec", kept_container, "cat", "/unity-install-location/extra") == b"kept"
    assert subprocess.run(["docker", "exec", kept_container, "test", "-e", "/home/desktop/Unity/Cache/extra"]).returncode != 0
    assert docker("exec", kept_container, "cat", "/home/desktop/Unity/Cache/marker") == b"cached"
    # Volumes created for the cache and the settings, as the dashboard does inline, and the
    # installs shared with the first container. Docker makes the folders above the cache and the
    # settings, which the desktop user gets; the session needs ~/.config before the setup runs,
    # so a root-owned one would leave XFCE without its settings and the desktop never ready.
    api("volumes", {"name": cache_volume})
    api("volumes", {"name": settings_volume})
    shared = job(api("containers", {**base, "name": "Shared installs", "parameters": {"UNITY_HOME": installs_volume, "UNITY_CACHE": cache_volume, "UNITY_SETTINGS": settings_volume}}))["container_id"]
    container_ids.append(shared)
    ready(f"bilskirnir-{shared}")
    assert volume_mounts(f"bilskirnir-{shared}") == {"/unity-install-location": installs_volume, "/home/desktop/Unity/Cache": cache_volume, "/home/desktop/.config/unity3d": settings_volume}
    assert docker("exec", f"bilskirnir-{shared}", "cat", "/unity-install-location/unity_v1") == f"/unity-install-location|{installs_volume}|/home/desktop/Unity/Cache|{cache_volume}|1001".encode()
    assert docker("exec", f"bilskirnir-{shared}", "stat", "-c", "%U", "/home/desktop/Unity", "/home/desktop/Unity/Cache", "/home/desktop/.config", "/home/desktop/.config/unity3d") == b"desktop\ndesktop\ndesktop\ndesktop\n"
    assert docker("exec", f"bilskirnir-{shared}", "pgrep", "-u", "desktop", "-x", "xfconfd")
    for eid in [kept, shared]:
        job(api(f"containers/{eid}", method="DELETE"))
        container_ids.remove(eid)
    assert docker("volume", "inspect", installs_volume) and docker("volume", "inspect", cache_volume) and docker("volume", "inspect", settings_volume)
    print("PASS volume parameters mount the chosen volume, keep it across settings and reset, and give the desktop user its folder", flush=True)
    # A volume parameter with new_volume gives each container a volume of its own: the creation
    # job makes it with its owner's labels, a reset and a settings change keep it, a snapshot's
    # container gets another one, the list names the owner, and a delete removes it only when
    # asked and never while another container mounts it.
    own_compose = {"parameters": [{"name": "WORK_SPACE", "label": "Workspace", "type": "volume", "path": "/home/desktop/Projects", "required": True, "new_volume": True}], "script": "set -eu\nprintf '%s|%s|%s' \"$WORK_SPACE\" \"$WORK_SPACE_VOLUME\" \"$(id -u)\" > \"$WORK_SPACE/owner\"\n"}
    for parameters in [[{**own_compose["parameters"][0], "default": share}], [{"name": "WORK_SPACE", "new_volume": True}]]:
        assert refused(f"images/{installs_image['id']}/compose", {**own_compose, "parameters": parameters}, "PUT") == 400, parameters
    own_image = job(api("images/build", {"name": "own-volume", "base_image": xfce["reference"], "internet": False, "script": f"# own volume {tag}\ntrue\n", "compose": own_compose}))
    assert own_image["compose"]["parameters"][0]["new_volume"] is True, own_image["compose"]
    own_base = {"image": own_image["image_id"], "internet": False}
    listed_names = lambda: {v["Name"] for v in api("volumes")}
    def deletion_refused(path):
        try:
            api(path, method="DELETE")
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read() or b"{}").get("error", "")
        raise AssertionError(f"DELETE {path} was accepted")
    before = listed_names()
    # A request refused before its job leaves no volume behind.
    assert refused("containers", {**own_base, "name": "own-refused", "mounts": [{"volume": share, "target": "/home/desktop/Projects"}]}) == 400
    # "-" asks for no volume, which a required parameter refuses before anything is created.
    assert refused("containers", {**own_base, "name": "own-none", "parameters": {"WORK_SPACE": "-"}}) == 400
    assert listed_names() == before
    created = api("containers", {**own_base, "name": "Own workspace"})
    own = job(created)["container_id"]
    container_ids.append(own)
    own_volume = f"bilskirnir-{own}-work-space"
    own_volumes.append(own_volume)
    assert f"Created the volume {own_volume} for WORK_SPACE." in api(f"jobs/{created['job_id']}")["log"]
    assert json.loads(docker("volume", "inspect", own_volume))[0]["Labels"] == {"bilskirnir.managed": "true", "bilskirnir.environment": own, "bilskirnir.parameter": "WORK_SPACE"}
    ready(f"bilskirnir-{own}")
    assert docker("exec", f"bilskirnir-{own}", "cat", "/home/desktop/Projects/owner") == f"/home/desktop/Projects|{own_volume}|1001".encode()
    assert api(f"containers/{own}")["setup"]["mounts"] == [{"volume": own_volume, "target": "/home/desktop/Projects", "readonly": False}]
    owner = {"container_id": own, "parameter": "WORK_SPACE", "name": "Own workspace", "deleted": False}
    assert next(v for v in api("volumes") if v["Name"] == own_volume)["owner"] == owner
    assert next(v for v in tool("list_volumes")["structuredContent"]["items"] if v["Name"] == own_volume)["owner"] == owner
    assert all(v["owner"] is None for v in api("volumes") if v["Name"] in (share, installs_volume))
    # Its owner's record mounts it for the compose, so it cannot be deleted by itself.
    status, message = deletion_refused(f"volumes/{own_volume}")
    assert status == 409 and message == '"Own workspace" mounts this volume for its compose; delete that container first', (status, message)
    docker("exec", "--user", "desktop", f"bilskirnir-{own}", "sh", "-c", "printf kept > /home/desktop/Projects/extra")
    job(api(f"containers/{own}/reset", {}))
    ready(f"bilskirnir-{own}")
    job(api(f"containers/{own}/settings", {"resolution": "1280x720"}))
    image_refs.append(api(f"containers/{own}")["working_image"]["reference"])
    ready(f"bilskirnir-{own}")
    assert volume_mounts(f"bilskirnir-{own}") == {"/home/desktop/Projects": own_volume}
    assert docker("exec", f"bilskirnir-{own}", "cat", "/home/desktop/Projects/extra") == b"kept"
    # A container from a snapshot gets a volume of its own. Deleted without the option, it
    # leaves that volume, which the list then shows with a deleted owner.
    own_snapshot = job(api(f"containers/{own}/snapshot", {"name": f"own volume {tag}"}))
    clone = job(api("containers", {**own_base, "image": own_snapshot["image_id"], "name": "Own clone"}))["container_id"]
    container_ids.append(clone)
    clone_volume = f"bilskirnir-{clone}-work-space"
    own_volumes.append(clone_volume)
    ready(f"bilskirnir-{clone}")
    assert volume_mounts(f"bilskirnir-{clone}") == {"/home/desktop/Projects": clone_volume}
    kept = job(api(f"containers/{clone}", method="DELETE"))
    container_ids.remove(clone)
    assert kept["volumes_preserved"] and "volumes_deleted" not in kept, kept
    assert next(v for v in api("volumes") if v["Name"] == clone_volume)["owner"] == {"container_id": clone, "parameter": "WORK_SPACE", "name": None, "deleted": True}
    # A container that names the volume shares it and gets none of its own, so deleting the
    # owner with the option keeps it, and deleting the sharer never takes a volume it chose.
    sharer = job(api("containers", {**own_base, "name": "Own sharer", "parameters": {"WORK_SPACE": own_volume}}))["container_id"]
    container_ids.append(sharer)
    assert json.loads(docker("volume", "inspect", f"bilskirnir-{sharer}-work-space", check=False) or b"[]") == []
    owner_deleted = mcp_job(tool("delete_container", {"container_id": own, "delete_volumes": True}))
    container_ids.remove(own)
    assert owner_deleted["result"]["volumes_kept"] == [own_volume] and owner_deleted["result"]["volumes_deleted"] == [], owner_deleted["result"]
    assert "Keeping the volume" in owner_deleted["log"] and "Own sharer" in owner_deleted["log"], owner_deleted["log"]
    sharer_deleted = job(api(f"containers/{sharer}?delete_volumes=true", method="DELETE"))
    container_ids.remove(sharer)
    assert sharer_deleted["volumes_deleted"] == [] and sharer_deleted["volumes_preserved"], sharer_deleted
    assert docker("volume", "inspect", own_volume)
    # Alone, its own volume goes with it when asked, and the volumes it did not own stay.
    alone = job(api("containers", {**own_base, "name": "Own alone"}))["container_id"]
    container_ids.append(alone)
    alone_volume = f"bilskirnir-{alone}-work-space"
    own_volumes.append(alone_volume)
    removed = job(api(f"containers/{alone}?delete_volumes=true", method="DELETE"))
    container_ids.remove(alone)
    assert removed["volumes_deleted"] == [alone_volume] and removed["volumes_kept"] == [] and not removed["volumes_preserved"], removed
    assert json.loads(docker("volume", "inspect", alone_volume, check=False) or b"[]") == []
    assert docker("volume", "inspect", own_volume) and docker("volume", "inspect", share)
    print("PASS new_volume gives each container its own labelled volume, kept across reset and settings, listed with its owner and deleted only on request", flush=True)
    # A volume is deleted by itself, such as the clone's, whose owner was deleted without it. It
    # answers at once, and refuses one a container record mounts, the manager's own, a missing
    # one and one any other container still uses, without removing anything.
    status, message = deletion_refused(f"volumes/{share}")
    assert status == 409 and message.startswith('"GPU test" mounts this volume; remove it from'), (status, message)
    refused_tool = tool("delete_volume", {"name": share}, allow_error=True)
    assert refused_tool["isError"] and '"GPU test"' in refused_tool["content"][0]["text"], refused_tool
    assert deletion_refused(f"volumes/{volume}")[0] == 403
    assert deletion_refused(f"volumes/{absent_volume}")[0] == 404
    assert json.loads(docker("volume", "inspect", absent_volume, check=False) or b"[]") == []
    assert deletion_refused("volumes/-bad")[0] == 400
    # Docker refuses a volume a container outside Bilskirnir uses, even a stopped one.
    docker("create", "--name", holder, "--label", f"bilskirnir.test={tag}", "--mount", f"type=volume,src={clone_volume},dst=/held", "--entrypoint", "true", xfce["reference"])
    status, message = deletion_refused(f"volumes/{clone_volume}")
    assert status == 409 and "volume is in use" in message, (status, message)
    docker("rm", holder)
    assert api(f"volumes/{clone_volume}", method="DELETE") == {"name": clone_volume, "deleted": True}
    assert json.loads(docker("volume", "inspect", clone_volume, check=False) or b"[]") == []
    assert clone_volume not in listed_names()
    # Once the sharer is gone nothing mounts the owner's volume either, and an agent deletes it.
    assert tool("delete_volume", {"name": own_volume})["structuredContent"] == {"name": own_volume, "deleted": True}
    assert json.loads(docker("volume", "inspect", own_volume, check=False) or b"[]") == []
    assert docker("volume", "inspect", share) and docker("volume", "inspect", volume)
    print("PASS a volume is deleted by itself only while nothing mounts it, never the manager's own or a missing one", flush=True)
    # Settings change after creation: the name and new limits apply to the running container, while
    # audio and removing a limit need a new container that keeps the old one's files.
    edit = job(api("containers", {"name": "Settings edit", "image": xfce["reference"], "gpu": False, "internet": False, "cpus": 1, "memory_mb": 1024}))["container_id"]
    container_ids.append(edit)
    edit_container = f"bilskirnir-{edit}"
    ready(edit_container)
    edit_identity = api(f"containers/{edit}")["spec"]
    first_id = docker("inspect", "--format", "{{.Id}}", edit_container).decode().strip()
    preview = api(f"containers/{edit}/settings", {"name": "Settings edited", "cpus": 2, "memory_mb": 2048, "dry_run": True})
    assert preview["live"] == ["name", "cpus", "memory_mb"] and preview["restart"] == [], preview
    in_place = api(f"containers/{edit}/settings", {"name": "Settings edited", "cpus": 2, "memory_mb": 2048})
    job(in_place)
    # Only a change that recreates the container reports stages.
    assert "stages" not in api(f"jobs/{in_place['job_id']}")
    limits = json.loads(docker("inspect", "--format", "{{json .HostConfig}}", edit_container))
    assert (limits["NanoCpus"], limits["Memory"], limits["MemorySwap"]) == (2 * 10**9, 2048 * 2**20, 4096 * 2**20), limits
    assert docker("inspect", "--format", "{{.Id}}", edit_container).decode().strip() == first_id
    assert api(f"containers/{edit}")["spec"]["name"] == "Settings edited"
    docker("exec", "--user", "desktop", edit_container, "sh", "-c", "printf kept > /home/desktop/settings-marker")
    preview = tool("update_container", {"container_id": edit, "audio": True, "cpus": None, "dry_run": True})["structuredContent"]
    assert preview["live"] == [] and preview["restart"] == ["cpus", "audio"], preview
    applied = mcp_job(tool("update_container", {"container_id": edit, "audio": True, "cpus": None}))
    assert "Open applications close" in applied["log"] and "Its files were kept" in applied["log"], applied["log"]
    # The running desktop was stopped first, so every stage ran, in order.
    assert [stage["name"] for stage in applied["stages"] if "started_at" in stage] == ["stop", "commit", "create", "cleanup"], applied["stages"]
    assert all(a["finished_at"] == b["started_at"] for a, b in zip(applied["stages"], applied["stages"][1:])), applied["stages"]
    assert "Stage 2/4: Committing the container layer\n" in applied["log"], applied["log"]
    carried =api(f"containers/{edit}")["working_image"]["reference"]
    image_refs.append(carried)
    ready(edit_container)
    second = json.loads(docker("inspect", edit_container))[0]
    assert second["Id"] != first_id and second["HostConfig"]["NanoCpus"] == 0, second["HostConfig"]
    assert "DESKTOP_AUDIO=true" in second["Config"]["Env"], second["Config"]["Env"]
    assert docker("exec", edit_container, "cat", "/home/desktop/settings-marker") == b"kept"
    assert b"Server Name" in docker("exec", "--user", "desktop", edit_container, "pactl", "info")
    assert not docker("ps", "-a", "--filter", f"name={edit_container}-previous", "--format", "{{.Names}}").strip()
    # The new container has the old one's identity; another identity is a change that recreates it.
    assert machine(edit_container) == recorded(edit_identity)
    takeover = {"hostname": "unity-build", "mac_address": "02-00-00-00-00-0A", "machine_id": "8AED3E38-BA43-5C50-DEAD-B0896ACE189B"}
    preview = api(f"containers/{edit}/settings", {**takeover, "dry_run": True})
    assert preview["live"] == [] and preview["restart"] == ["hostname", "mac_address", "machine_id"], preview
    assert refused(f"containers/{edit}/settings", {"hostname": None}) == 400
    assert refused(f"containers/{edit}/settings", {"mac_address": "01:00:5e:00:00:01"}) == 400
    job(api(f"containers/{edit}/settings", takeover))
    image_refs.append(api(f"containers/{edit}")["working_image"]["reference"])
    ready(edit_container)
    second = json.loads(docker("inspect", edit_container))[0]
    taken = {"hostname": "unity-build", "mac_address": "02:00:00:00:00:0a", "machine_id": "8aed3e38ba435c50deadb0896ace189b"}
    assert {key: api(f"containers/{edit}")["spec"][key] for key in taken} == taken
    assert machine(edit_container) == recorded(taken)
    assert docker("exec", edit_container, "cat", "/home/desktop/settings-marker") == b"kept"
    print("PASS machine identity survives a settings recreation and moves to the values a takeover sets", flush=True)
    # A new container would get the manager's data volume, so the change stops nothing.
    refused_mount = mcp_job(tool("update_container", {"container_id": edit, "mounts": [{"volume": volume, "target": "/workspace"}]}), success=False)
    assert f"{volume} is the manager's own volume" in refused_mount["log"], refused_mount["log"]
    assert docker("inspect", "--format", "{{.Id}} {{.State.Running}}", edit_container).decode().split() == [second["Id"], "true"]
    assert api(f"containers/{edit}")["spec"]["mounts"] == []
    deletion = job(api(f"containers/{edit}", method="DELETE"))
    container_ids.remove(edit)
    assert deletion["deleted"] == edit and json.loads(docker("image", "inspect", carried, check=False) or b"[]") == []
    print("PASS settings change in place, recreation that keeps files, and carried image cleanup", flush=True)
    for eid in list(container_ids):
        deletion = api(f"containers/{eid}", method="DELETE")
        result = job(deletion)
        container_ids.remove(eid)
        log = api(f"jobs/{deletion['job_id']}")["log"]
        assert "Checking ownership" in log and "Removed the container and its writable layer" in log, log
        assert "Named volumes and saved images were preserved" in log, log
        assert result["name"] and result["volumes_preserved"] and result["images_preserved"], result
    assert not api("containers")
    # A deleted container keeps its usage history, marked removed, until its samples age out.
    kept = next((c for c in api("usage")["containers"] if c["id"] == env_id), None)
    assert kept and kept["removed"] is True and kept["samples"], kept
    assert api(f"containers/{env_id}/usage")["removed"] is True
    assert tool("get_usage", {"container_id": env_id, "seconds": 1800})["structuredContent"]["containers"][0]["removed"] is True
    assert docker("volume", "inspect", share)
    # Without a GPU container the manager stops sampling the GPU, and its helper goes.
    wait_for(lambda: not set(docker("ps", "-aq", "--filter", "label=bilskirnir.usage-helper=true").decode().split()) - foreign_helpers, 30)
    assert api("usage")["gpu"]["state"] == "idle"
    print("PASS container cleanup preserves volumes and the deleted containers' usage history", flush=True)
    print("ALL DOCKER ACCEPTANCE CHECKS PASSED", flush=True)
finally:
    # Discover partial containers even if their failed create response did not return an ID.
    try:
        for item in api("containers"):
            eid=item["id"]
            if eid not in container_ids: container_ids.append(eid)
    except Exception:
        pass
    def inspect_cleanup(kind,name):
        data=docker(kind,"inspect",name,check=False)
        values=json.loads(data) if data else []
        return values[0] if values else None
    # Remove only this test's known resources, after verifying ownership.
    info=inspect_cleanup("container",foreign)
    if info and info["Config"]["Labels"].get("bilskirnir.test")==tag:
        docker("rm","--force",foreign,check=False)
    info=inspect_cleanup("container",holder)
    if info and info["Config"]["Labels"].get("bilskirnir.test")==tag:
        docker("rm","--force",holder,check=False)
    image_fixture=inspect_cleanup("container",f"bilskirnir-test-image-{tag}")
    if image_fixture and image_fixture["Config"]["Labels"].get("bilskirnir.test")==tag:
        docker("rm","--force",f"bilskirnir-test-image-{tag}",check=False)
    for eid in container_ids:
        # A settings change interrupted by a failure can leave the replaced container renamed aside.
        for name in [f"bilskirnir-{eid}",f"bilskirnir-{eid}-previous"]:
            info=inspect_cleanup("container",name)
            if info and info["Config"]["Labels"].get("bilskirnir.environment")==eid:
                docker("rm","--force",info["Id"],check=False)
        for network in [f"bilskirnir-{eid}-private",f"bilskirnir-{eid}-egress"]:
            info=inspect_cleanup("network",network)
            if info and (info["Labels"].get("bilskirnir.environment")==eid or info["Labels"].get("bilskirnir.test")==tag):
                docker("network","disconnect","--force",info["Id"],manager,check=False)
                docker("network","rm",info["Id"],check=False)
    docker("rm","--force",manager,check=False)
    for name in [volume,share,clone_share,files_volume,restored_volume,streamed_volume,cut_volume,interrupted_volume,copied_volume,mcp_copy_volume,device_volume,device_copy_volume,linked_volume,installs_volume,cache_volume,settings_volume,absent_volume,outside_volume]: docker("volume","rm",name,check=False)
    # A volume created for one of this run's containers goes only while its label names that container.
    for name in own_volumes:
        info=inspect_cleanup("volume",name)
        if info and (info["Labels"] or {}).get("bilskirnir.environment")==name.split("-")[1]:
            docker("volume","rm",name,check=False)
    for reference in reversed(image_refs): docker("image","rm",reference,check=False)
