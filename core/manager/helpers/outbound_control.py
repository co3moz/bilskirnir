"""Ask the desktop's proxy how it is, or give it the container's settings; runs as root.

    python3 outbound_control.py DIRECTORY status
    python3 outbound_control.py DIRECTORY apply < settings.json

`status` first makes DIRECTORY, a folder under the desktop's tmpfs that only nobody, the proxy's
user, may open, so the proxy can start there. The settings arrive on stdin and go to the proxy's
control socket, never to a file or a command line.
"""
import json
import os
import socket
import sys

NOBODY = 65534

directory, command = sys.argv[1], sys.argv[2]
path = os.path.join(directory, "control")


def prepare():
    os.makedirs(directory, 0o700, exist_ok=True)
    if os.path.islink(directory) or not os.path.isdir(directory):
        raise RuntimeError(directory + " is not a folder")
    os.chown(directory, NOBODY, NOBODY)
    os.chmod(directory, 0o700)


def ask(request):
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(10)
    try:
        connection.connect(path)
    except OSError:
        connection.close()
        return None
    with connection:
        connection.sendall((json.dumps(request) + "\n").encode())
        reply = b""
        while not reply.endswith(b"\n"):
            data = connection.recv(65536)
            if not data:
                break
            reply += data
    return json.loads(reply)


def absent():
    try:
        with open(os.path.join(directory, "error")) as handle:
            return {"state": "failed", "error": handle.read(1000)}
    except OSError:
        return {"state": "absent"}


try:
    if command == "status":
        prepare()
        print(json.dumps(ask({"command": "status"}) or absent()))
    elif command == "apply":
        settings = json.load(sys.stdin)
        print(json.dumps(ask({"command": "apply", "settings": settings}) or absent()))
    else:
        sys.exit(2)
except Exception as error:
    print("Proxy operation failed: " + str(error), file=sys.stderr)
    sys.exit(1)
