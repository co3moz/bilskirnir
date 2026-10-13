"""Clean one interrupted acceptance run, identified by its exact manager name."""
import json
import re
import subprocess
import sys

def docker(*args):
    return subprocess.check_output(["docker", *args], timeout=120)

def inspect(kind, name):
    result = subprocess.run(["docker", kind, "inspect", name], capture_output=True, timeout=120)
    values = json.loads(result.stdout) if result.returncode == 0 else []
    return values[0] if values else None

manager = sys.argv[1]
match = re.fullmatch(r"wsld-test-([a-f0-9]{12})", manager)
assert match, "Exact acceptance-test manager name required"
tag = match[1]
info = inspect("container", manager)
assert info and info["Config"]["Labels"].get("io.wsl-desktop.manager") == "true"
assert f"MANAGER_CONTAINER={manager}" in info["Config"]["Env"]
volume = f"wsld-test-data-{tag}"
assert any(m.get("Name") == volume and m["Destination"] == "/data" for m in info["Mounts"])
records = json.loads(docker("run", "--rm", "--entrypoint", "python3", "--mount", f"type=volume,src={volume},dst=/data", "bilskirnir/manager:local", "-c", "import sqlite3,json; c=sqlite3.connect('file:/data/state.sqlite?mode=ro',uri=True); print(json.dumps([(kind,json.loads(data)) for kind,data in c.execute(\"SELECT kind,data FROM records WHERE kind IN ('environment','image')\")]))"))
for kind, record in records:
    if kind != "environment":
        continue
    eid = record["id"]
    assert re.fullmatch(r"[a-f0-9]{32}", eid)
    container = inspect("container", f"wsld-{eid}")
    if container:
        assert container["Config"]["Labels"].get("io.wsl-desktop.environment") == eid
        assert container["Config"]["Labels"].get("io.wsl-desktop.managed") == "true"
        docker("rm", "--force", container["Id"])
    for suffix in ("private", "egress"):
        network = inspect("network", f"wsld-{eid}-{suffix}")
        if network:
            assert network["Labels"].get("io.wsl-desktop.environment") == eid or network["Labels"].get("io.wsl-desktop.test") == tag
            if info["Id"] in network.get("Containers", {}):
                docker("network", "disconnect", "--force", network["Id"], info["Id"])
            docker("network", "rm", network["Id"])
docker("rm", "--force", info["Id"])
for name in (volume, f"wsld-test-share-{tag}", f"wsld-test-clone-{tag}"):
    if inspect("volume", name):
        docker("volume", "rm", name)
for kind, record in reversed(records):
    if kind == "image" and record["kind"] in ("build", "snapshot"):
        image = inspect("image", record["reference"])
        if image:
            assert image["Id"] == record["image_id"] and image["Config"]["Labels"].get("io.wsl-desktop.managed") == "true"
            docker("image", "rm", record["reference"])
print(f"Cleaned interrupted acceptance run {manager}")
