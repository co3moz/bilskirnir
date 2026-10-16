"""Run a container's template setup script, started by the manager through docker exec.

Reads one JSON request from stdin: the script, the parameter values, the names of the secret
parameters and the time limit. Each parameter becomes an environment variable of the script,
which runs with fail-on-error and stdin closed. Its output is streamed back with every secret
value replaced by ***, and the exit status is the script's.
"""

import json
import os
import selectors
import signal
import subprocess
import sys
import tempfile
import time

REQUEST_LIMIT = 8 << 20
MASK = b"***"


class Masker:
    """Masks secrets in a stream, holding back the bytes a secret split across reads could start in."""

    def __init__(self, secrets):
        self.secrets = sorted({s for s in secrets if s}, key=len, reverse=True)
        self.hold = max((len(s) for s in self.secrets), default=1) - 1
        self.pending = b""

    def mask(self, data):
        for secret in self.secrets:
            data = data.replace(secret, MASK)
        return data

    def push(self, chunk):
        self.pending = self.mask(self.pending + chunk)
        cut = len(self.pending) - self.hold
        if cut <= 0:
            return b""
        out, self.pending = self.pending[:cut], self.pending[cut:]
        return out

    def finish(self):
        out, self.pending = self.mask(self.pending), b""
        return out


def emit(data):
    if data:
        sys.stdout.buffer.write(data)
        sys.stdout.buffer.flush()


def stop(process):
    """Ends the script and whatever it started, which share its session."""
    for sig, wait in ((signal.SIGTERM, 5), (signal.SIGKILL, 5)):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            return
        try:
            process.wait(wait)
            return
        except subprocess.TimeoutExpired:
            pass


def main():
    request = json.loads(sys.stdin.buffer.read(REQUEST_LIMIT))
    values = {str(k): str(v) for k, v in request.get("parameters", {}).items()}
    secrets = [values.get(name, "").encode() for name in request.get("secrets", [])]
    timeout = int(request.get("timeout_seconds", 600))
    environment = dict(os.environ)
    environment.update(values)
    # An unlinked copy: a command that reads stdin cannot consume the script, and nothing of it
    # stays behind in the desktop.
    with tempfile.NamedTemporaryFile("wb", prefix="bilskirnir-setup.", delete=False) as handle:
        handle.write(request["script"].encode())
        path = handle.name
    try:
        script = os.open(path, os.O_RDONLY)
    finally:
        os.unlink(path)
    process = subprocess.Popen(
        ["bash", "-e", f"/dev/fd/{script}"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=environment,
        pass_fds=(script,),
        start_new_session=True,
    )
    os.close(script)
    masker = Masker(secrets)
    output = process.stdout.fileno()
    os.set_blocking(output, False)
    selector = selectors.DefaultSelector()
    selector.register(output, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    exited_at = None
    while True:
        now = time.monotonic()
        if now >= deadline:
            emit(masker.finish())
            emit(f"\nThe setup script did not finish within {timeout} seconds and was stopped.\n".encode())
            stop(process)
            return 124
        if exited_at is None and process.poll() is not None:
            exited_at = now
        # A program the script left running can hold the output open; stop reading soon after
        # the script itself exits.
        if exited_at is not None and now - exited_at > 1:
            break
        if not selector.select(min(deadline - now, 0.2)):
            continue
        try:
            chunk = os.read(output, 65536)
        except BlockingIOError:
            continue
        if not chunk:
            break
        emit(masker.push(chunk))
    emit(masker.finish())
    return process.wait()


if __name__ == "__main__":
    sys.exit(main())
