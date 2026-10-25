"""Report, feed or stop the virtual gamepad daemon; runs as root like the daemon."""
import json
import os
import sys
import time

directory, command = sys.argv[1], sys.argv[2]


def status():
    try:
        with open(os.path.join(directory, "status")) as handle:
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
    descriptor = os.open(os.path.join(directory, "input"), os.O_WRONLY)
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
        if os.path.isdir(directory):
            with open(os.path.join(directory, "stop"), "w"):
                pass
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and status().get("state") in ("running", "starting"):
            time.sleep(0.1)
        print(json.dumps(status()))
    else:
        sys.exit(2)
except Exception as error:
    print("Gamepad operation failed: " + str(error), file=sys.stderr)
    sys.exit(1)
