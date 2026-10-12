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
    docker("run", "-d", "--name", manager, "--label", "io.wsl-desktop.manager=true", "-e", f"MANAGER_CONTAINER={manager}", "-e", "AUTH_MODE=password", "-e", f"ADMIN_PASSWORD={password}", "-e", "GPU_ADAPTER=NVIDIA", "-v", "/var/run/docker.sock:/var/run/docker.sock", "-v", f"{volume}:/data", "-p", f"127.0.0.1:{port or ''}:8080", "bilskirnir/manager:local")
    return int(docker("port", manager, "8080/tcp").decode().strip().rsplit(":", 1)[1])


def login():
    wait_for(lambda: api("session", anonymous=True))
    api("login", {"username": "admin", "password": password})


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
    assert api("system")["connected"]
    profiles = api("profiles")
    assert {p["profile"]["session"] for p in profiles} == {"xfce", "gnome"}
    assert all(p["available"] for p in profiles), profiles
    xfce = next(p for p in profiles if p["profile"]["session"] == "xfce")
    gnome = next(p for p in profiles if p["profile"]["session"] == "gnome")
    # A profile build through the manager must work from its bundled Docker context.
    assert job(api("profiles/ubuntu-xfce/build", {}))["profile_id"] == "ubuntu-xfce"
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
