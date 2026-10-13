"""Write to, read from or stop an interactive terminal session run by the terminal runner."""
import base64
import json
import os
import re
import sys
import time

ESCAPES = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]")


def status(directory):
    try:
        with open(os.path.join(directory, "status")) as handle:
            return json.load(handle)
    except (OSError, ValueError):
        if os.path.isdir(directory):
            return {"state": "starting", "written": 0, "base": 0}
        return {"state": "unknown", "written": 0, "base": 0,
                "error": "Terminal files are not available; the desktop may have restarted"}


def read_file(path):
    try:
        with open(path, "rb") as handle:
            return handle.read()
    except OSError:
        return b""


def write(directory, encoded):
    info = status(directory)
    if info.get("state") != "running":
        raise RuntimeError("Terminal is not running")
    descriptor = os.open(os.path.join(directory, "input"), os.O_WRONLY)
    try:
        os.write(descriptor, base64.b64decode(encoded))
    finally:
        os.close(descriptor)
    return status(directory)


def read(directory, offset, limit, strip):
    info = status(directory)
    old = read_file(os.path.join(directory, "output.1"))
    new = read_file(os.path.join(directory, "output"))
    written = info.get("written", 0)
    start = written - len(old) - len(new)
    dropped = offset < start
    offset = max(start, min(offset, written))
    chunk = (old + new)[offset - start:offset - start + limit]
    text = chunk.decode("utf-8", "replace")
    if strip:
        text = ESCAPES.sub("", text).replace("\r", "")
    result = dict(info)
    result.update({"output": text, "offset": offset, "next_offset": offset + len(chunk),
                   "more": offset + len(chunk) < written, "dropped": dropped})
    return result


def stop(directory, wait_seconds):
    if os.path.isdir(directory):
        with open(os.path.join(directory, "stop"), "w"):
            pass
    deadline = time.monotonic() + wait_seconds
    while time.monotonic() < deadline and status(directory).get("state") in ("running", "starting"):
        time.sleep(0.1)
    return status(directory)


command, directory, *arguments = sys.argv[1:]
try:
    if command == "status":
        print(json.dumps(status(directory)))
    elif command == "write":
        print(json.dumps(write(directory, arguments[0])))
    elif command == "read":
        print(json.dumps(read(directory, int(arguments[0]), int(arguments[1]), arguments[2] == "true")))
    elif command == "stop":
        print(json.dumps(stop(directory, float(arguments[0]))))
    else:
        sys.exit(2)
except Exception as error:
    print("Terminal operation failed: " + str(error), file=sys.stderr)
    sys.exit(1)
