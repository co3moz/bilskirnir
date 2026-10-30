"""Create a virtual Xbox-style gamepad with uinput and apply commands from a FIFO; runs as root, in
a folder only root opens."""
import fcntl
import json
import os
import select
import stat
import struct
import sys
import time

directory = sys.argv[1]
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
BUTTONS = {"a": 0x130, "b": 0x131, "x": 0x133, "y": 0x134, "lb": 0x136, "rb": 0x137, "back": 0x13a,
           "start": 0x13b, "guide": 0x13c, "ls": 0x13d, "rs": 0x13e}
AXES = {"left_x": (0, -32768, 32767), "left_y": (1, -32768, 32767), "right_x": (3, -32768, 32767),
        "right_y": (4, -32768, 32767), "left_trigger": (2, 0, 255), "right_trigger": (5, 0, 255),
        "dpad_x": (0x10, -1, 1), "dpad_y": (0x11, -1, 1)}
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502
UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT = 0x40045564, 0x40045565, 0x40045567
UI_GET_SYSNAME = (2 << 30) | (64 << 16) | (0x55 << 8) | 0x2C

def private_folder(path):
    """Make PATH a folder only this user opens, or take the one it made before. One another user
    made, or a link, is refused: the desktop user can make either in /tmp before a root helper."""
    try:
        os.mkdir(path, 0o700)
    except FileExistsError:
        pass
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        if os.fstat(descriptor).st_uid != os.geteuid():
            raise RuntimeError(path + " belongs to another user")
        os.fchmod(descriptor, 0o700)
    finally:
        os.close(descriptor)


private_folder(directory)
fifo = os.path.join(directory, "input")
if not os.path.exists(fifo):
    os.mkfifo(fifo, 0o600)
state = {"state": "starting", "pid": os.getpid(), "device": None, "nodes": [], "buttons": {}, "axes": {},
         "error": None}


def record():
    temporary = os.path.join(directory, "status.tmp")
    with open(temporary, "w") as handle:
        json.dump(state, handle)
    os.replace(temporary, os.path.join(directory, "status"))


record()
null = os.open(os.devnull, os.O_RDWR)
for descriptor in (0, 1, 2):
    os.dup2(null, descriptor)
os.close(null)
nodes = []
device = None
try:
    device = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    for kind in (EV_KEY, EV_ABS):
        fcntl.ioctl(device, UI_SET_EVBIT, kind)
    for code in BUTTONS.values():
        fcntl.ioctl(device, UI_SET_KEYBIT, code)
    absmax, absmin, absfuzz, absflat = [0] * 64, [0] * 64, [0] * 64, [0] * 64
    for code, low, high in AXES.values():
        fcntl.ioctl(device, UI_SET_ABSBIT, code)
        absmin[code], absmax[code] = low, high
        if high == 32767:
            absfuzz[code], absflat[code] = 16, 128
    setup = (b"Thoric Gamepad".ljust(80, b"\0") + struct.pack("<HHHHI", 3, 0x045E, 0x028E, 0x110, 0)
             + struct.pack("<64i", *absmax) + struct.pack("<64i", *absmin)
             + struct.pack("<64i", *absfuzz) + struct.pack("<64i", *absflat))
    os.write(device, setup)
    fcntl.ioctl(device, UI_DEV_CREATE)
    time.sleep(0.5)
    buffer = bytearray(64)
    fcntl.ioctl(device, UI_GET_SYSNAME, buffer)
    sysname = bytes(buffer).split(b"\0")[0].decode()
    os.makedirs("/dev/input", 0o755, exist_ok=True)
    base = "/sys/devices/virtual/input/" + sysname
    for entry in sorted(os.listdir(base)):
        if entry.startswith("event") or entry.startswith("js"):
            with open(os.path.join(base, entry, "dev")) as handle:
                major, minor = handle.read().strip().split(":")
            path = "/dev/input/" + entry
            if not os.path.exists(path):
                os.mknod(path, 0o666 | stat.S_IFCHR, os.makedev(int(major), int(minor)))
            os.chmod(path, 0o666)
            nodes.append(path)
    state.update({"state": "running", "nodes": nodes,
                  "device": next((node for node in nodes if "event" in node), None)})
    record()
except Exception as error:
    state.update({"state": "failed", "error": str(error)})
    record()
    sys.exit(1)


def emit(events):
    data = b"".join(struct.pack("<qqHHi", 0, 0, kind, code, value) for kind, code, value in events)
    os.write(device, data + struct.pack("<qqHHi", 0, 0, EV_SYN, 0, 0))


def apply(command):
    if command.get("reset"):
        events = [(EV_KEY, code, 0) for code in BUTTONS.values()]
        events += [(EV_ABS, code, 0) for code, _, _ in AXES.values()]
        state["buttons"], state["axes"] = {}, {}
    elif "button" in command:
        code = BUTTONS[command["button"]]
        pressed = bool(command.get("pressed", True))
        events = [(EV_KEY, code, 1 if pressed else 0)]
        state["buttons"][command["button"]] = pressed
    elif "axis" in command:
        code, low, high = AXES[command["axis"]]
        value = max(-1.0, min(1.0, float(command["value"])))
        if low < 0:
            raw = int(round(value * high))
        else:
            raw = int(round(max(value, 0.0) * high))
        events = [(EV_ABS, code, raw)]
        state["axes"][command["axis"]] = value
    else:
        return
    emit(events)
    record()


reader = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
pending = b""
while not os.path.exists(os.path.join(directory, "stop")):
    if select.select([reader], [], [], 0.5)[0]:
        try:
            pending += os.read(reader, 65536)
        except BlockingIOError:
            pass
    while b"\n" in pending:
        line, pending = pending.split(b"\n", 1)
        try:
            apply(json.loads(line))
        except (ValueError, KeyError, OSError):
            continue
try:
    fcntl.ioctl(device, UI_DEV_DESTROY)
except OSError:
    pass
for path in nodes:
    try:
        os.unlink(path)
    except OSError:
        pass
state.update({"state": "exited", "device": None, "nodes": []})
record()
