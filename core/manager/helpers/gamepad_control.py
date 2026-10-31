"""Report, feed or stop the virtual gamepad daemon; runs as root like the daemon.

The daemon's folder counts only while it is root's own, as the daemon makes it, and its files are
opened without following links, so nothing the desktop user prepared in /tmp is touched."""
import json
import os
import stat
import sys
import time

directory, command = sys.argv[1], sys.argv[2]


def ours():
    try:
        info = os.lstat(directory)
    except OSError:
        return False
    return stat.S_ISDIR(info.st_mode) and info.st_uid == os.geteuid()


def opened(name, flags=os.O_RDONLY):
    return os.open(os.path.join(directory, name), flags | os.O_NOFOLLOW, 0o600)


def status():
    try:
        if not ours():
            raise OSError("not the daemon's folder")
        with os.fdopen(opened("status")) as handle:
            info = json.load(handle)
    except (OSError, ValueError):
        return {"state": "absent", "device": None, "buttons": {}, "axes": {}}
    if info.get("state") == "running":
        try:
            os.kill(int(info["pid"]), 0)
        except (ProcessLookupError, ValueError, TypeError):
            info["state"] = "exited"
    return info


def send(actions):
    descriptor = opened("input", os.O_WRONLY)
    applied = 0
    try:
        for action in actions:
            if action.get("type") == "wait":
                time.sleep(min(float(action.get("milliseconds", 0)), 10000) / 1000)
            else:
                os.write(descriptor, (json.dumps(action) + "\n").encode())
            applied += 1
    finally:
        os.close(descriptor)
    time.sleep(0.05)
    return applied


try:
    if command == "status":
        print(json.dumps(status()))
    elif command == "send":
        info = status()
        if info.get("state") != "running":
            raise RuntimeError("Gamepad device is not running")
        applied = send(json.load(sys.stdin))
        result = status()
        result["applied_actions"] = applied
        print(json.dumps(result))
    elif command == "stop":
        if ours():
            os.close(opened("stop", os.O_WRONLY | os.O_CREAT))
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and status().get("state") in ("running", "starting"):
            time.sleep(0.1)
        print(json.dumps(status()))
    else:
        sys.exit(2)
except Exception as error:
    print("Gamepad operation failed: " + str(error), file=sys.stderr)
    sys.exit(1)
