"""Report, read logs of, stop and measure applications supervised by the runner; control recordings."""
import json
import os
import signal
import sys
import time


def read(directory, name):
    try:
        with open(os.path.join(directory, name)) as handle:
            return handle.read().strip()
    except OSError:
        return None


def status(directory):
    result = {"state": "unknown", "pid": None, "exit_code": None, "signal": None, "error": None,
              "started_at": None, "log_bytes": 0}
    if not os.path.isdir(directory):
        result["error"] = "Application files are not available; the desktop may have restarted"
        return result
    for name in ("log", "log.1"):
        try:
            result["log_bytes"] += os.path.getsize(os.path.join(directory, name))
        except OSError:
            pass
    pid, state, started = read(directory, "pid"), read(directory, "status"), read(directory, "started_at")
    if pid:
        result["pid"] = int(pid)
    if started:
        result["started_at"] = int(started)
    if state:
        kind, _, detail = state.partition(" ")
        if kind == "exited":
            code = int(detail)
            result["state"] = "exited"
            if code < 0:
                result["signal"] = -code
            else:
                result["exit_code"] = code
        else:
            result["state"] = "failed"
            result["error"] = detail
    elif pid:
        try:
            os.kill(int(pid), 0)
            result["state"] = "running"
        except ProcessLookupError:
            result["error"] = "Process ended without a recorded status"
        except PermissionError:
            result["state"] = "running"
    else:
        result["state"] = "starting"
    return result


def logs(directory, tail):
    data = b""
    for name in ("log.1", "log"):
        try:
            with open(os.path.join(directory, name), "rb") as handle:
                data += handle.read()
        except OSError:
            pass
    return {"log": data[-tail:].decode("utf-8", "replace"), "bytes": min(len(data), tail),
            "total_bytes": len(data), "truncated": len(data) > tail}


def stop(directory, grace):
    info = status(directory)
    pid = info["pid"]
    if info["state"] == "running" and pid:
        try:
            os.killpg(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline and status(directory)["state"] == "running":
            time.sleep(0.1)
        if status(directory)["state"] == "running":
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            for _ in range(50):
                if status(directory)["state"] != "running":
                    break
                time.sleep(0.1)
    return status(directory)


def processes(session):
    """Processes in the application's session (the runner starts it as a session leader)."""
    items = []
    page = os.sysconf("SC_PAGE_SIZE")
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            with open("/proc/%s/stat" % pid) as handle:
                text = handle.read()
            with open("/proc/%s/cmdline" % pid, "rb") as handle:
                command = handle.read().replace(b"\0", b" ").strip().decode("utf-8", "replace")
        except OSError:
            continue
        name_end = text.rfind(")")
        fields = text[name_end + 2:].split()
        if len(fields) < 22 or int(fields[3]) != session:
            continue
        items.append({"pid": int(pid), "ppid": int(fields[1]), "state": fields[0],
                      "cpu_ticks": int(fields[11]) + int(fields[12]),
                      "rss_bytes": int(fields[21]) * page,
                      "command": command or text[text.find("(") + 1:name_end]})
    return sorted(items, key=lambda item: item["pid"])


def diagnostics(directory):
    info = status(directory)
    result = dict(info)
    result.update({"processes": [], "open_files": None, "threads": None,
                   "log_tail": logs(directory, 4096)["log"]})
    pid = info["pid"]
    if pid and info["state"] == "running":
        result["processes"] = [{key: value for key, value in item.items() if key != "cpu_ticks"}
                               for item in processes(pid)]
        try:
            result["open_files"] = len(os.listdir("/proc/%d/fd" % pid))
        except OSError:
            pass
        try:
            with open("/proc/%d/status" % pid) as handle:
                for line in handle:
                    if line.startswith("Threads:"):
                        result["threads"] = int(line.split()[1])
        except OSError:
            pass
    return result


def cgroup(name):
    try:
        with open("/sys/fs/cgroup/" + name) as handle:
            value = handle.read().strip()
        return None if value == "max" else int(value)
    except (OSError, ValueError):
        return None


def metrics(directory, duration_ms):
    info = status(directory)
    pid = info["pid"] if info["state"] == "running" else None

    def snapshot():
        return {item["pid"]: item for item in processes(pid)} if pid else {}

    before = snapshot()
    started = time.monotonic()
    time.sleep(duration_ms / 1000)
    after = snapshot()
    elapsed = time.monotonic() - started
    ticks = sum(after[p]["cpu_ticks"] - before[p]["cpu_ticks"] if p in before else after[p]["cpu_ticks"]
                for p in after)
    hz = os.sysconf("SC_CLK_TCK")
    return {"state": status(directory)["state"], "duration_ms": int(elapsed * 1000),
            "cpu_percent": round(ticks / hz / elapsed * 100, 1) if elapsed and pid else None,
            "rss_bytes": sum(item["rss_bytes"] for item in after.values()),
            "processes": len(after),
            "container_memory_bytes": cgroup("memory.current"),
            "container_memory_limit_bytes": cgroup("memory.max"),
            "fps": None}


def recording_status(directory):
    data = read(directory, "status")
    if data:
        return json.loads(data)
    if os.path.isdir(directory):
        return {"state": "starting", "frames": 0, "bytes": 0}
    return {"state": "unknown", "frames": 0, "bytes": 0,
            "error": "Recording files are not available; the desktop may have restarted"}


def recording_stop(directory, wait_seconds):
    if os.path.isdir(directory):
        with open(os.path.join(directory, "stop"), "w"):
            pass
    deadline = time.monotonic() + wait_seconds
    while time.monotonic() < deadline and recording_status(directory).get("state") in ("recording", "starting"):
        time.sleep(0.1)
    return recording_status(directory)


command, *arguments = sys.argv[1:]
if command == "status":
    print(json.dumps([status(directory) for directory in arguments]))
elif command == "logs":
    print(json.dumps(logs(arguments[0], int(arguments[1]))))
elif command == "stop":
    print(json.dumps(stop(arguments[0], float(arguments[1]))))
elif command == "diagnostics":
    print(json.dumps(diagnostics(arguments[0])))
elif command == "metrics":
    print(json.dumps(metrics(arguments[0], int(arguments[1]))))
elif command == "recording-status":
    print(json.dumps([recording_status(directory) for directory in arguments]))
elif command == "recording-stop":
    print(json.dumps(recording_stop(arguments[0], float(arguments[1]))))
else:
    sys.exit(2)
