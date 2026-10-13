#!/usr/bin/env python3
"""Real Docker acceptance test. Run inside WSL after building both images.
Uses unique resources, checks ownership on cleanup, never prunes the daemon.
"""
import base64
import http.cookiejar
import json
import math
import os
import socket
import struct
import subprocess
import time
import tempfile
import urllib.error
import urllib.request
import urllib.parse
import http.client
import uuid


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
manager = f"wsld-test-{tag}"
volume = f"wsld-test-data-{tag}"
share = f"wsld-test-share-{tag}"
clone_share = f"wsld-test-clone-{tag}"
password = uuid.uuid4().hex
mcp_token = uuid.uuid4().hex + uuid.uuid4().hex
environment_ids = []
image_refs = []
foreign = f"wsld-test-foreign-{tag}"
port = None
cookies = http.cookiejar.CookieJar()
client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookies))


def api(path, body=None, method=None, raw=False, anonymous=False):
    if path == "images/build" and body:
        body = {**body, "script": f"# acceptance run {tag}\n" + body["script"]}
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/{path}", data=None if body is None else json.dumps(body).encode(), method=method, headers={"Content-Type": "application/json", "X-Wsld-Request": "1"})
    opener = urllib.request.build_opener() if anonymous else client
    with opener.open(request, timeout=130) as response:
        data = response.read()
        return data if raw else (json.loads(data) if data else None)


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
    if result["result"] and "reference" in result["result"] and "profile_id" not in result["result"]:
        image_refs.append(result["result"]["reference"])
    return result["result"]


def start_manager():
    docker("run", "-d", "--name", manager, "--label", "io.wsl-desktop.manager=true", "-e", f"MANAGER_CONTAINER={manager}", "-e", "AUTH_MODE=password", "-e", f"ADMIN_PASSWORD={password}", "-e", "MCP_ENABLED=true", "-e", f"MCP_TOKEN={mcp_token}", "-e", "GPU_ADAPTER=NVIDIA", "-v", "/var/run/docker.sock:/var/run/docker.sock", "-v", f"{volume}:/data", "-p", f"127.0.0.1:{port or ''}:8080", "bilskirnir/manager:local")
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


def mcp_job(response, success=True):
    jid = response["structuredContent"]["job_id"]
    result = wait_for(lambda: (j if (j := tool("get_job", {"job_id":jid})["structuredContent"])["status"] not in ("queued","running") else None), 180)
    assert result["status"] == ("completed" if success else "failed"), result
    if success and result["result"] and result["result"].get("reference") and "profile_id" not in result["result"]:
        image_refs.append(result["result"]["reference"])
    return result


def upload(eid, name, data, destination="/home/desktop/Downloads", replace=False):
    query = urllib.parse.urlencode({"destination":destination,"name":name,"size":len(data),"replace":str(replace).lower()})
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/environments/{eid}/files?{query}",data=data,headers={"Content-Type":"application/octet-stream","X-Wsld-Request":"1"})
    with client.open(request,timeout=120) as response:
        return json.loads(response.read())


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


try:
    port = start_manager()
    login()
    try:
        api("environments", anonymous=True)
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
    assert len(mcp("tools/list")["tools"]) == 47
    assert tool("get_system")["structuredContent"]["connected"]
    assert stdio_tool("get_system")["structuredContent"]["connected"]
    print("PASS MCP authentication, negotiation and discovery", flush=True)
    assert api("system")["connected"]
    profiles = api("profiles")
    assert {p["profile"]["session"] for p in profiles} == {"xfce", "gnome"}
    assert all(p["available"] for p in profiles), profiles
    xfce = next(p for p in profiles if p["profile"]["session"] == "xfce")
    gnome = next(p for p in profiles if p["profile"]["session"] == "gnome")
    # A profile build through the manager must work from its bundled Docker context.
    assert mcp_job(tool("build_profile", {"profile_id":"ubuntu-xfce"}))["result"]["profile_id"] == "ubuntu-xfce"
    print("PASS distinct desktop profile catalog and built-in image preparation", flush=True)
    api("volumes", {"name": share})
    api("volumes", {"name": clone_share})
    try:
        api("volumes", {"name": share}); raise AssertionError("duplicate volume accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 409
    for target in ["//etc", "/./etc", "/var/run", "/home"]:
        try:
            api("environments", {"name":"invalid-mount","image":xfce["profile"]["reference"],"mounts":[{"volume":share,"target":target}]})
            raise AssertionError(f"unsafe mount accepted: {target}")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/session", headers={"Host":"evil.example","Origin":"http://evil.example"})
    try:
        client.open(request); raise AssertionError("untrusted Host accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 403
    print("PASS duplicate volume, mount aliases and Host validation", flush=True)
    built = job(api("images/build", {"name": "offline-script-test", "base_image": xfce["profile"]["reference"], "internet": False, "script": "set -eu\nprintf script-built > /opt/script-built\npython3 - <<'PY'\nimport socket\ns = socket.socket(); s.settimeout(2)\ntry:\n s.connect(('1.1.1.1',443))\nexcept OSError:\n pass\nelse:\n raise RuntimeError('offline build had internet')\nPY\n"}))
    print("PASS custom script image, offline build and persisted build log", flush=True)
    spec = {"name": "GPU test", "image": built["image_id"], "resolution": "1280x720", "gpu": True, "internet": False, "audio": True, "microphone": True, "cpus": 2, "memory_mb": 2048, "mounts": [{"volume": share, "target": "/shared", "readonly": False}]}
    env_id = job(api("environments", spec))["environment_id"]
    environment_ids.append(env_id)
    container = f"wsld-{env_id}"
    ready(container)
    default_exec = mcp_job(stdio_tool("exec_environment", {"environment_id":env_id,"script":"id -u\nprintf agent-install > /home/desktop/agent-marker"}))
    assert "1001" in default_exec["log"] and default_exec["result"]["user"] == "1001:1001"
    root_exec = mcp_job(tool("exec_environment", {"environment_id":env_id,"root":True,"script":"id -u\ninstall -m 644 /home/desktop/agent-marker /opt/agent-install"}))
    assert root_exec["result"]["user"] == "root"
    assert docker("exec",container,"cat","/opt/agent-install") == b"agent-install"
    failure = mcp_job(tool("exec_environment", {"environment_id":env_id,"script":"echo EXPECTED_EXEC_FAILURE\nexit 17"}), success=False)
    assert "EXPECTED_EXEC_FAILURE" in failure["log"] and "17" in failure["log"]
    timeout = mcp_job(tool("exec_environment", {"environment_id":env_id,"script":"sleep 5\nprintf must-not-exist > /home/desktop/after-timeout","timeout_seconds":1}), success=False)
    assert "124" in timeout["log"]
    time.sleep(1)
    assert docker("exec",container,"test","!","-e","/home/desktop/after-timeout") == b""
    written = stdio_tool("write_file", {"environment_id":env_id,"path":"/home/desktop/agent-files/hello.txt","data":"Hello from MCP\n"})["structuredContent"]
    assert written["bytes"] == 15
    read = api(f"environments/{env_id}/files/read", {"path":written["path"]})
    assert read["data"] == "Hello from MCP\n" and read["eof"]
    assert docker("exec",container,"stat","-c","%u:%g %a",written["path"]).strip() == b"1001:1001 600"
    conflict = tool("write_file", {"environment_id":env_id,"path":written["path"],"data":"no overwrite"}, allow_error=True)
    assert conflict["isError"]
    api(f"environments/{env_id}/files/write", {"path":written["path"],"data":"Updated","replace":True})
    page = tool("read_file", {"environment_id":env_id,"path":written["path"],"offset":2,"limit":3})["structuredContent"]
    assert page["data"] == "dat" and page["next_offset"] == 5 and not page["eof"]
    binary = bytes(range(256))
    tool("write_file", {"environment_id":env_id,"path":"/home/desktop/agent-files/agent.bin","encoding":"base64","data":base64.b64encode(binary).decode()})
    assert base64.b64decode(tool("read_file", {"environment_id":env_id,"path":"/home/desktop/agent-files/agent.bin","encoding":"base64"})["structuredContent"]["data"]) == binary
    assert tool("read_file", {"environment_id":env_id,"path":"/dev/zero"}, allow_error=True)["isError"]
    tool("upload_file", {"environment_id":env_id,"destination":"/home/desktop/Downloads","name":"agent-upload.bin","data_base64":base64.b64encode(binary).decode()})
    assert docker("exec",container,"cat","/home/desktop/Downloads/agent-upload.bin") == binary
    image = stdio_tool("screenshot", {"environment_id":env_id})["content"][0]
    assert image["type"] == "image" and image["mimeType"] == "image/jpeg" and base64.b64decode(image["data"]).startswith(b"\xff\xd8")
    assert tool("read_file", {"environment_id":env_id,"path":"/home/desktop/../etc/passwd"}, allow_error=True)["isError"]
    docker("exec", "--user", "desktop", container, "ln", "-s", "/home/desktop/agent-files", "/home/desktop/agent-link")
    assert tool("write_file", {"environment_id":env_id,"path":"/home/desktop/agent-link/blocked.txt","data":"blocked"}, allow_error=True)["isError"]
    assert tool("read_file", {"environment_id":env_id,"path":"/home/desktop/agent-link/hello.txt"}, allow_error=True)["isError"]
    print("PASS MCP/HTTP execution, exit failures, timeout, bounded file read/write, upload and screenshot", flush=True)
    docker("exec", "--detach", "--user", "desktop", "--env", "DISPLAY=:0", container, "bash", "-c", 'exec xev -geometry 320x180+80+80 -name "Bilskirnir input test" -event mouse -event keyboard > /home/desktop/input-events.log 2>&1')
    def input_position():
        info = docker("exec", "--env", "DISPLAY=:0",container,"xwininfo","-name","Bilskirnir input test").decode()
        import re
        x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info)[1]) + 160
        y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info)[1]) + 120
        return x,y
    x,y = wait_for(input_position)
    viewer = WebSocket(port, f"/api/environments/{env_id}/vnc", "; ".join(f"{c.name}={c.value}" for c in cookies))
    assert viewer.read(12).startswith(b"RFB ")
    viewer.send(b"RFB 003.008\n")
    types = viewer.read(viewer.read(1)[0]); assert 1 in types
    viewer.send(b"\x01"); assert viewer.read(4)==b"\0"*4
    viewer.send(b"\x01"); init=viewer.read(24); viewer.read(int.from_bytes(init[20:24],"big"))
    result = stdio_tool("desktop_input", {"environment_id":env_id,"actions":[{"type":"move","x":x,"y":y},{"type":"click","x":x,"y":y},{"type":"type_text","text":"agent"},{"type":"key_press","keys":["Control","a"]},{"type":"scroll","x":x,"y":y,"direction":"up","steps":2}]})["structuredContent"]
    assert result["applied_actions"] == 5 and result["width"]==1280 and result["height"]==720
    api(f"environments/{env_id}/input", {"actions":[{"type":"drag","from_x":x,"from_y":y,"to_x":x+20,"to_y":y+10},{"type":"key_press","keys":["Escape"]}]})
    events = wait_for(lambda: (data if b"KeyPress" in (data:=docker("exec",container,"cat","/home/desktop/input-events.log")) and b"ButtonPress" in data and b"Control_L" in data else None))
    assert b"button 1" in events and b"button 4" in events and b"Escape" in events, events
    viewer.send(b"\x03\x00" + struct.pack("!HHHH",0,0,1,1))
    assert viewer.read(4)[0] == 0, "Agent input disconnected the existing VNC viewer"
    viewer.close()
    rejected = tool("desktop_input", {"environment_id":env_id,"actions":[{"type":"click","x":2000,"y":y}]}, allow_error=True)
    assert rejected["isError"]
    print("PASS MCP/HTTP shared VNC mouse, keyboard, drag, scroll, Fence acknowledgement and bounds", flush=True)
    # Script stdin is closed: a command reading input cannot consume the rest of the script.
    isolated = mcp_job(tool("exec_environment", {"environment_id":env_id,"script":"consumed=$(cat); echo \"stdin-bytes=${#consumed}\"\necho AFTER_STDIN\n"}))
    assert "stdin-bytes=0" in isolated["log"] and "AFTER_STDIN" in isolated["log"], isolated["log"]
    launched = tool("launch_application", {"environment_id":env_id,"command":"bash","args":["-c","echo APP_LOG_LINE; exec xlogo -title 'Bilskirnir app test' -geometry 200x150+600+300"],"name":"xlogo test"})["structuredContent"]
    app_id = launched["application_id"]
    assert launched["state"] in ("running","starting"), launched
    running_app = wait_for(lambda: (s if (s := tool("get_application", {"environment_id":env_id,"application_id":app_id})["structuredContent"])["state"] == "running" else None))
    assert running_app["pid"], running_app
    window = wait_for(lambda: next((w for w in tool("list_windows", {"environment_id":env_id})["structuredContent"]["items"] if w["title"] == "Bilskirnir app test"), None))
    tool("focus_window", {"environment_id":env_id,"window_id":window["window_id"]})
    geometry = tool("set_window_geometry", {"environment_id":env_id,"window_id":window["window_id"],"x":100,"y":120,"width":320,"height":240})["structuredContent"]
    assert geometry["width"] == 320 and geometry["height"] == 240, geometry
    moved = wait_for(lambda: next((w for w in tool("list_windows", {"environment_id":env_id})["structuredContent"]["items"] if w["window_id"] == window["window_id"] and w["width"] == 320), None))
    assert moved["active"], moved
    full = tool("screenshot", {"environment_id":env_id})["structuredContent"]
    assert full["width"] == 1280 and full["height"] == 720, full
    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/environments/{env_id}/screenshot?max_width=640", headers={"X-Wsld-Request":"1"})
    with client.open(request, timeout=60) as response:
        assert response.headers["X-Image-Width"] == "640" and response.read().startswith(b"\xff\xd8")
    region = tool("screenshot", {"environment_id":env_id,"region":"100,120,320,240","max_width":160})["structuredContent"]
    assert region["width"] == 160 and region["height"] == 120, region
    captured = tool("screenshot", {"environment_id":env_id,"window_id":window["window_id"]})["structuredContent"]
    assert captured["width"] == 320 and captured["height"] == 240, captured
    observed = tool("act_and_observe", {"environment_id":env_id,"actions":[{"type":"move","x":x,"y":y},{"type":"move_relative","dx":10,"dy":5},{"type":"hold_keys","keys":["w"],"milliseconds":300},{"type":"key_down","keys":["Shift"]}],"settle_ms":200,"capture":{"max_width":320}})
    assert observed["content"][0]["type"] == "image" and observed["structuredContent"]["width"] == 320, observed["structuredContent"]
    assert observed["structuredContent"]["input"]["released"]["keys"] == ["Shift"], observed["structuredContent"]
    assert observed["structuredContent"]["input"]["pointer"] == {"x":x+10,"y":y+5}, observed["structuredContent"]
    location = docker("exec","--user","desktop","--env","DISPLAY=:0",container,"xdotool","getmouselocation","--shell").decode()
    assert f"X={x+10}\n" in location and f"Y={y+5}\n" in location, location
    relative_only = api(f"environments/{env_id}/input", {"actions":[{"type":"move_relative","dx":-10,"dy":-5}]})
    assert relative_only["pointer"] == {"x":x,"y":y}, relative_only
    logs = tool("application_logs", {"environment_id":env_id,"application_id":app_id,"tail_bytes":4096})["structuredContent"]
    assert "APP_LOG_LINE" in logs["log"], logs
    stopped = tool("stop_application", {"environment_id":env_id,"application_id":app_id,"grace_seconds":3})["structuredContent"]
    assert stopped["state"] == "exited" and stopped["signal"] == 15, stopped
    assert any(a["application_id"] == app_id and a["state"] == "exited" for a in api(f"environments/{env_id}/applications")["items"])
    missing_program = tool("launch_application", {"environment_id":env_id,"command":"/nonexistent/program"})["structuredContent"]
    assert missing_program["state"] == "failed", missing_program
    print("PASS application lifecycle, window management, sized screenshots, held keys and relative pointer", flush=True)
    second = tool("launch_application", {"environment_id":env_id,"command":"bash","args":["-c","echo READY_MARKER; exec xlogo -title 'Bilskirnir wait test'"]})["structuredContent"]["application_id"]
    waited = tool("wait_for_condition", {"environment_id":env_id,"window_title":"Bilskirnir wait test","application_id":second,"application_state":"running","log_contains":"READY_MARKER","timeout_ms":15000})["structuredContent"]
    assert waited["satisfied"] and waited["window"]["title"] == "Bilskirnir wait test" and waited["log_found"], waited
    stable = api(f"environments/{env_id}/wait", {"screen_stable_ms":600,"region":"900,500,200,120","timeout_ms":10000,"interval_ms":200})
    assert stable["satisfied"] and stable["screen_stable_for_ms"] >= 600, stable
    unmet = tool("wait_for_condition", {"environment_id":env_id,"window_title":"no such window","timeout_ms":600,"interval_ms":200})["structuredContent"]
    assert not unmet["satisfied"] and unmet["window"] is None, unmet
    diagnostics = tool("application_diagnostics", {"environment_id":env_id,"application_id":second})["structuredContent"]
    assert diagnostics["processes"] and "READY_MARKER" in diagnostics["log_tail"] and diagnostics["open_files"], diagnostics
    sampled = tool("sample_metrics", {"environment_id":env_id,"application_id":second,"duration_ms":300})["structuredContent"]
    assert sampled["cpu_percent"] is not None and sampled["rss_bytes"] > 0 and sampled["processes"] >= 1, sampled
    recording = tool("start_recording", {"environment_id":env_id,"fps":4,"max_seconds":20,"max_width":320,"name":"wait test"})["structuredContent"]
    time.sleep(1.5)
    finished = tool("stop_recording", {"environment_id":env_id,"recording_id":recording["recording_id"]})["structuredContent"]
    assert finished["state"] == "finished" and finished["frames"] >= 2 and finished["reason"] == "stopped", finished
    assert any(r["recording_id"] == recording["recording_id"] and r["frames"] == finished["frames"] for r in tool("list_recordings", {"environment_id":env_id})["structuredContent"]["items"])
    frames = tool("list_files", {"environment_id":env_id,"path":finished["directory"]})["structuredContent"]
    assert sum(1 for f in frames["items"] if f["name"].endswith(".jpg")) == finished["frames"], frames
    exported = tool("export_artifacts", {"environment_id":env_id,"destination":"/home/desktop/Downloads","name":"evidence.tar.gz","recording_ids":[recording["recording_id"]],"application_ids":[second]})["structuredContent"]
    listing = docker("exec", container, "tar", "-tzf", exported["path"]).decode()
    assert "manifest.json" in listing and f"recordings/{recording['recording_id']}/frame-00000" in listing and f"applications/{second}/log" in listing, listing
    assert tool("export_artifacts", {"environment_id":env_id,"destination":"/home/desktop/Downloads","name":"evidence.tar.gz","recording_ids":[recording["recording_id"]]}, allow_error=True)["isError"]
    tool("move_file", {"environment_id":env_id,"source":"/home/desktop/agent-files/hello.txt","destination":"/home/desktop/agent-files/renamed.txt"})
    entries = tool("list_files", {"environment_id":env_id,"path":"/home/desktop/agent-files"})["structuredContent"]["items"]
    assert any(e["name"] == "renamed.txt" and e["type"] == "file" for e in entries) and not any(e["name"] == "hello.txt" for e in entries), entries
    assert tool("list_files", {"environment_id":env_id,"path":"/home/desktop/agent-link"}, allow_error=True)["isError"]
    assert tool("delete_file", {"environment_id":env_id,"path":"/home/desktop/agent-files"}, allow_error=True)["isError"]
    tool("delete_file", {"environment_id":env_id,"path":"/home/desktop/agent-files","recursive":True})
    assert docker("exec",container,"test","!","-e","/home/desktop/agent-files") == b""
    tool("stop_application", {"environment_id":env_id,"application_id":second})
    print("PASS waits, diagnostics, metrics, frame recordings, artifact export and file tools", flush=True)
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
    docker("exec",container,"chown","1001:1001","/shared")
    volume_upload = upload(env_id,"project.txt",b"persistent-upload",destination="/shared/uploads")
    assert docker("exec",container,"cat",volume_upload["path"]) == b"persistent-upload"
    docker("exec",container,"ln","-s","/tmp","/home/desktop/Downloads/escape")
    for target in ["/etc", "/home/desktop/Downloads/../.ssh", "/home/desktop/Downloads/escape"]:
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
    transfer.putrequest("POST",f"/api/environments/{env_id}/files?{query}")
    transfer.putheader("Content-Type","application/octet-stream")
    transfer.putheader("Content-Length",10*1024*1024)
    transfer.putheader("X-Wsld-Request","1")
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
    print("PASS binary/empty uploads, desktop ownership, volume persistence, overwrite protection, symlinks and revoked transfer cleanup",flush=True)
    assert docker("exec", container, "cat", "/opt/script-built") == b"script-built"
    diag = api(f"environments/{env_id}/diagnostics")["output"]
    assert "D3D12 (NVIDIA" in diag, diag
    assert "Default Source: browser-microphone" in diag, diag
    print("PASS independent desktop, hardware OpenGL and virtual audio devices", flush=True)
    jpeg = api(f"environments/{env_id}/screenshot", raw=True)
    assert jpeg.startswith(b"\xff\xd8") and len(jpeg) > 1000
    cookie = "; ".join(f"{c.name}={c.value}" for c in cookies)
    ws = WebSocket(port, f"/api/environments/{env_id}/vnc", cookie)
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
        WebSocket(port, f"/api/environments/{env_id}/vnc", cookie, "https://evil.example")
        raise AssertionError("cross-origin VNC accepted")
    except RuntimeError as error:
        assert "403" in str(error)
    print("PASS JPEG preview, authenticated VNC handshake and origin restriction", flush=True)
    output = WebSocket(port, f"/api/environments/{env_id}/audio/output", cookie)
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
    mic = WebSocket(port, f"/api/environments/{env_id}/audio/input", cookie)
    mic.send(struct.pack("<" + "f" * 4096, *([0.1] * 4096)))
    live_vnc = WebSocket(port, f"/api/environments/{env_id}/vnc", cookie)
    assert live_vnc.read(12).startswith(b"RFB 003.")
    live_output = WebSocket(port, f"/api/environments/{env_id}/audio/output", cookie)
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
    job(api(f"environments/{env_id}/internet", {"enabled": True}))
    assert subprocess.run(["docker", "exec", container, "python3", "-c", offline_script], capture_output=True).returncode == 0
    job(api(f"environments/{env_id}/internet", {"enabled": False}))
    assert api(f"environments/{env_id}/screenshot", raw=True).startswith(b"\xff\xd8")
    assert subprocess.run(["docker", "exec", container, "python3", "-c", offline_script], capture_output=True).returncode != 0
    print("PASS live internet on/off with VNC still reachable", flush=True)
    docker("exec", "--user", "desktop", container, "bash", "-c", "printf snapshot-home > /home/desktop/snapshot-marker; printf volume-data > /shared/volume-marker")
    snap = job(api(f"environments/{env_id}/snapshot", {"name": "snapshot-test"}))
    assert snap["desktop"]["profile"] == "ubuntu-xfce"
    assert snap["parent_image_id"] == built["image_id"]
    base_layers = json.loads(docker("image", "inspect", built["image_id"]))[0]["RootFS"]["Layers"]
    snap_layers = json.loads(docker("image", "inspect", snap["image_id"]))[0]["RootFS"]["Layers"]
    assert snap_layers[:len(base_layers)] == base_layers and len(snap_layers) > len(base_layers)
    spec.update({"name": "Clone", "image": snap["image_id"], "gpu": False, "audio": False, "microphone": False, "mounts": [{"volume": clone_share, "target": "/shared"}]})
    clone_id = job(api("environments", spec))["environment_id"]
    environment_ids.append(clone_id)
    clone = f"wsld-{clone_id}"
    ready(clone)
    assert docker("exec", clone, "cat", "/home/desktop/snapshot-marker") == b"snapshot-home"
    assert subprocess.run(["docker", "exec", clone, "test", "-e", "/shared/volume-marker"]).returncode != 0
    print("PASS layered snapshot, immutable ancestry and independent volume clone", flush=True)
    slow = api("images/build", {"name":"concurrent-build","base_image":xfce["profile"]["reference"],"internet":False,"script":"set -eu\nprintf SLOW_STARTED\nsleep 20\nprintf SLOW_FINISHED\n"})
    wait_for(lambda: "SLOW_STARTED" in api(f"jobs/{slow['job_id']}")["log"])
    started = time.monotonic()
    job(api(f"environments/{clone_id}/action", {"action": "stop"}))
    assert time.monotonic() - started < 8, "stop blocked by unrelated build"
    assert api(f"jobs/{slow['job_id']}")["status"] == "running"
    job(slow)
    print("PASS long image build does not block other environment mutations",flush=True)
    job(api(f"environments/{clone_id}/action", {"action": "start"}))
    ready(clone)
    assert docker("exec", clone, "cat", "/home/desktop/snapshot-marker") == b"snapshot-home"
    # Delete preflight must leave the writable layer intact when a network has a foreign endpoint.
    docker("run","-d","--name",foreign,"--label",f"io.wsl-desktop.test={tag}","--network",f"wsld-{clone_id}-egress","--entrypoint","sleep",xfce["profile"]["reference"],"300")
    deletion = api(f"environments/{clone_id}", method="DELETE")
    failed = wait_for(lambda: (j if (j := api(f"jobs/{deletion['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"] == "failed",failed
    assert docker("exec",clone,"cat","/home/desktop/snapshot-marker") == b"snapshot-home"
    docker("rm","--force",foreign)
    print("PASS delete preflight preserves container when network has another endpoint",flush=True)
    # Recover a manually removed container using its existing networks and pinned source image.
    docker("rm","--force",clone)
    docker("volume","rm",clone_share)
    missing_volume=api(f"environments/{clone_id}/retry",{})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{missing_volume['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "volume" in failed["log"].lower(),failed
    assert json.loads(docker("volume","inspect",clone_share,check=False))==[]
    api("volumes",{"name":clone_share})
    job(api(f"environments/{clone_id}/retry",{}));ready(clone)
    assert docker("exec",clone,"cat","/home/desktop/snapshot-marker") == b"snapshot-home"
    print("PASS missing container retry reuses environment identity and immutable image",flush=True)
    # One malformed environment must not prevent other desktops from reconnecting after restart.
    corrupt_network=f"wsld-{clone_id}-egress"
    docker("network","rm",corrupt_network)
    docker("network","create","--label",f"io.wsl-desktop.test={tag}",corrupt_network)
    docker("rm", "--force", manager)
    start_manager(); cookies.clear(); login()
    wait_for(lambda: api(f"environments/{env_id}/screenshot", raw=True).startswith(b"\xff\xd8"), 50)
    assert len(api("environments")) == 2
    docker("network","rm",corrupt_network)
    job(api(f"environments/{clone_id}/retry",{}))
    print("PASS malformed environment network does not block reconciliation of others",flush=True)
    print("PASS stop/start persistence and manager replacement/network reconciliation", flush=True)
    gnome_spec = {"name":"GNOME GPU test","image":gnome["profile"]["reference"],"gpu":True,"internet":False,"memory_mb":3072,"cpus":2,"audio":True,"microphone":True,"resolution":"1280x720"}
    gnome_id=job(api("environments",gnome_spec))["environment_id"]
    environment_ids.append(gnome_id)
    gnome_container=f"wsld-{gnome_id}"
    ready(gnome_container)
    assert b"GNOME Shell 46" in docker("exec",gnome_container,"gnome-shell","--version")
    assert subprocess.run(["docker","exec",gnome_container,"pgrep","-x","gnome-shell"],capture_output=True).returncode==0
    assert subprocess.run(["docker","exec",gnome_container,"pgrep","-f","^/usr/libexec/gnome-session-failed"],capture_output=True).returncode!=0
    assert api(f"environments/{gnome_id}/screenshot",raw=True).startswith(b"\xff\xd8")
    assert "D3D12 (NVIDIA" in api(f"environments/{gnome_id}/diagnostics")["output"]
    gnome_snapshot=job(api(f"environments/{gnome_id}/snapshot",{"name":"GNOME snapshot"}))
    assert gnome_snapshot["desktop"]["session"] == "gnome"
    assert gnome_snapshot["parent_image_id"] == gnome["image_id"]
    gnome_spec.update({"name":"GNOME CPU clone","image":gnome_snapshot["image_id"],"gpu":False,"audio":False,"microphone":False})
    gnome_clone_id=job(api("environments",gnome_spec))["environment_id"]
    environment_ids.append(gnome_clone_id)
    ready(f"wsld-{gnome_clone_id}")
    assert subprocess.run(["docker","exec",f"wsld-{gnome_clone_id}","pgrep","-x","gnome-shell"],capture_output=True).returncode==0
    assert subprocess.run(["docker","exec",f"wsld-{gnome_clone_id}","pgrep","-f","^/usr/libexec/gnome-session-failed"],capture_output=True).returncode!=0
    docker("exec","-d","--user","desktop",f"wsld-{gnome_clone_id}","bash","-c","exec -a /usr/libexec/gnome-session-failed sleep 30")
    assert subprocess.run(["docker","exec",f"wsld-{gnome_clone_id}","/usr/local/bin/desktop-healthcheck"],capture_output=True).returncode!=0
    print("PASS GNOME Shell GPU desktop, preview, snapshot metadata and CPU clone",flush=True)
    # An OCI start failure leaves a discoverable container/intent that retry can repair in place.
    broken_reference=f"bilskirnir/test-broken:{tag}"
    fixture_container=f"wsld-test-image-{tag}"
    docker("create","--name",fixture_container,"--label",f"io.wsl-desktop.test={tag}",xfce["profile"]["reference"])
    docker("commit","--change",'ENTRYPOINT ["/usr/local/bin/desktop-missing-entrypoint"]',fixture_container,broken_reference)
    image_refs.append(broken_reference)
    volume_reference=f"bilskirnir/test-volume:{tag}"
    docker("commit","--change",'VOLUME ["/home/desktop"]',fixture_container,volume_reference)
    image_refs.append(volume_reference);docker("rm",fixture_container)
    before=len(api("environments"))
    implicit_volume=api("environments",{"name":"implicit-volume","image":volume_reference,"internet":False})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{implicit_volume['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "VOLUME" in failed["log"],failed
    assert len(api("environments"))==before
    print("PASS implicit image VOLUME cannot silently exclude home from snapshots",flush=True)
    broken={"image_id":json.loads(docker("image","inspect",broken_reference))[0]["Id"]}
    create_failed = api("environments",{"name":"repairable-start","image":broken["image_id"],"internet":False,"gpu":False})
    failed = wait_for(lambda: (j if (j := api(f"jobs/{create_failed['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"] == "failed",failed
    repair = next(x["environment"] for x in api("environments") if x["environment"]["spec"]["name"]=="repairable-start")
    repair_id=repair["id"];environment_ids.append(repair_id);repair_container=repair["container"]
    assert repair["last_error"]
    previous_id=json.loads(docker("inspect",repair_container))[0]["Id"]
    with tempfile.TemporaryDirectory() as directory:
        restored=os.path.join(directory,"desktop-entrypoint")
        with open(restored,"wb") as file:
            file.write(docker("run","--rm","--entrypoint","cat",xfce["profile"]["reference"],"/usr/local/bin/desktop-entrypoint"))
        os.chmod(restored,0o755)
        docker("cp",restored,f"{repair_container}:/usr/local/bin/desktop-missing-entrypoint")
    job(api(f"environments/{repair_id}/retry",{}));ready(repair_container)
    assert json.loads(docker("inspect",repair_container))[0]["Id"]==previous_id
    assert not next(x["environment"] for x in api("environments") if x["environment"]["id"]==repair_id)["last_error"]
    print("PASS failed setup retry repairs existing container without discarding its layer",flush=True)
    # Binary / very long script output cannot abort or exhaust manager log buffers.
    noisy=api("images/build",{"name":"binary-long-log","base_image":xfce["profile"]["reference"],"internet":False,"script":"python3 -c \"import sys; sys.stdout.buffer.write(bytes([255])+b'x'*300000)\"\n"})
    job(noisy)
    assert len(api(f"jobs/{noisy['job_id']}")["log"].encode())<=262144
    assert docker("exec",manager,"test","!","-e",f"/data/builds/{noisy['job_id']}")==b""
    assert not docker("image","ls","-q",f"bilskirnir/build-base:{noisy['job_id']}")
    print("PASS bounded binary build logs and temporary script/tag cleanup",flush=True)
    bad=api("images/build",{"name":"failed-script","base_image":xfce["profile"]["reference"],"internet":False,"script":"echo EXPECTED_SCRIPT_FAILURE\nexit 17\n"})
    failed=wait_for(lambda: (j if (j := api(f"jobs/{bad['job_id']}"))["status"] not in ("queued","running") else None))
    assert failed["status"]=="failed" and "EXPECTED_SCRIPT_FAILURE" in failed["log"],failed
    assert docker("exec",manager,"test","!","-e",f"/data/builds/{bad['job_id']}")==b""
    assert not docker("image","ls","-q",f"bilskirnir/build-base:{bad['job_id']}")
    print("PASS failed build also removes temporary scripts and pinned base tag",flush=True)
    agent_image = mcp_job(tool("build_image", {"name":"agent-build","base_image":xfce["profile"]["reference"],"internet":False,"script":f"# acceptance {tag}\nprintf mcp-built > /opt/mcp-built"}))["result"]
    agent_env = mcp_job(tool("create_environment", {"name":"Agent lifecycle","image":agent_image["image_id"],"internet":False,"gpu":False}))["result"]["environment_id"]
    environment_ids.append(agent_env)
    ready(f"wsld-{agent_env}")
    assert tool("get_environment", {"environment_id":agent_env})["structuredContent"]["state"] == "running"
    assert tool("read_file", {"environment_id":agent_env,"path":"/opt/mcp-built"})["structuredContent"]["data"] == "mcp-built"
    tool("write_file", {"environment_id":agent_env,"path":"/home/desktop/from-agent.txt","data":"snapshot from MCP"})
    agent_snapshot = mcp_job(tool("snapshot", {"environment_id":agent_env,"name":"Agent snapshot"}))["result"]
    assert agent_snapshot["parent_image_id"] == agent_image["image_id"]
    assert any(image["id"] == agent_snapshot["id"] for image in tool("list_images")["structuredContent"]["items"])
    reset_job = mcp_job(tool("reset_environment", {"environment_id":agent_env}))
    assert reset_job["result"]["volumes_preserved"] and "Recreated the desktop" in reset_job["log"], reset_job
    ready(f"wsld-{agent_env}")
    assert tool("read_file", {"environment_id":agent_env,"path":"/home/desktop/from-agent.txt"}, allow_error=True)["isError"]
    assert tool("read_file", {"environment_id":agent_env,"path":"/opt/mcp-built"})["structuredContent"]["data"] == "mcp-built"
    print("PASS environment reset discards the container layer and keeps the pinned image", flush=True)
    mcp_job(tool("set_internet", {"environment_id":agent_env,"enabled":True}))
    mcp_job(tool("set_internet", {"environment_id":agent_env,"enabled":False}))
    mcp_job(tool("environment_action", {"environment_id":agent_env,"action":"stop"}))
    mcp_job(tool("environment_action", {"environment_id":agent_env,"action":"start"}))
    ready(f"wsld-{agent_env}")
    deleted = mcp_job(tool("delete_environment", {"environment_id":agent_env}))
    assert deleted["result"]["volumes_preserved"] and deleted["result"]["images_preserved"]
    assert "Named volumes and saved images were preserved" in deleted["log"]
    environment_ids.remove(agent_env)
    clone = mcp_job(tool("create_environment", {"name":"Agent snapshot clone","image":agent_snapshot["image_id"],"gpu":False,"internet":False}))["result"]["environment_id"]
    environment_ids.append(clone)
    ready(f"wsld-{clone}")
    assert tool("read_file", {"environment_id":clone,"path":"/home/desktop/from-agent.txt"})["structuredContent"]["data"] == "snapshot from MCP"
    mcp_job(tool("delete_environment", {"environment_id":clone}))
    environment_ids.remove(clone)
    print("PASS complete MCP image build, desktop create, snapshot, clone, internet, lifecycle and deletion", flush=True)
    for eid in list(environment_ids):
        deletion = api(f"environments/{eid}", method="DELETE")
        result = job(deletion)
        environment_ids.remove(eid)
        log = api(f"jobs/{deletion['job_id']}")["log"]
        assert "Checking ownership" in log and "Removed the desktop container" in log, log
        assert "Named volumes and saved images were preserved" in log, log
        assert result["name"] and result["volumes_preserved"] and result["images_preserved"], result
    assert not api("environments")
    assert docker("volume", "inspect", share)
    print("PASS environment cleanup preserves volumes", flush=True)
    print("ALL DOCKER ACCEPTANCE CHECKS PASSED", flush=True)
finally:
    # Discover partial environments even if their failed create response did not return an ID.
    try:
        for item in api("environments"):
            eid=item["environment"]["id"]
            if eid not in environment_ids: environment_ids.append(eid)
    except Exception:
        pass
    def inspect_cleanup(kind,name):
        data=docker(kind,"inspect",name,check=False)
        values=json.loads(data) if data else []
        return values[0] if values else None
    # Remove only this test's known resources, after verifying ownership.
    info=inspect_cleanup("container",foreign)
    if info and info["Config"]["Labels"].get("io.wsl-desktop.test")==tag:
        docker("rm","--force",foreign,check=False)
    image_fixture=inspect_cleanup("container",f"wsld-test-image-{tag}")
    if image_fixture and image_fixture["Config"]["Labels"].get("io.wsl-desktop.test")==tag:
        docker("rm","--force",f"wsld-test-image-{tag}",check=False)
    for eid in environment_ids:
        name=f"wsld-{eid}"
        info=inspect_cleanup("container",name)
        if info and info["Config"]["Labels"].get("io.wsl-desktop.environment")==eid:
            docker("rm","--force",info["Id"],check=False)
        for network in [f"wsld-{eid}-private",f"wsld-{eid}-egress"]:
            info=inspect_cleanup("network",network)
            if info and (info["Labels"].get("io.wsl-desktop.environment")==eid or info["Labels"].get("io.wsl-desktop.test")==tag):
                docker("network","disconnect","--force",info["Id"],manager,check=False)
                docker("network","rm",info["Id"],check=False)
    docker("rm","--force",manager,check=False)
    for name in [volume,share,clone_share]: docker("volume","rm",name,check=False)
    for reference in reversed(image_refs): docker("image","rm",reference,check=False)
