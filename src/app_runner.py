"""Supervise one desktop application: bounded log, pid file and recorded exit status."""
import os
import subprocess
import sys
import time

directory, command = sys.argv[1], sys.argv[2:]
LIMIT = 1024 * 1024
os.makedirs(directory, 0o700, exist_ok=True)


def record(name, text):
    temporary = os.path.join(directory, name + ".tmp")
    with open(temporary, "w") as handle:
        handle.write(text + "\n")
    os.replace(temporary, os.path.join(directory, name))


# Detach from the docker exec stream so the supervisor outlives the request that started it.
null = os.open(os.devnull, os.O_RDWR)
for descriptor in (0, 1, 2):
    os.dup2(null, descriptor)
os.close(null)
log_path = os.path.join(directory, "log")
log = open(log_path, "ab", buffering=0)
size = 0
try:
    child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, start_new_session=True)
except OSError as error:
    record("status", "failed " + str(error))
    sys.exit(1)
record("pid", str(child.pid))
record("started_at", str(int(time.time())))
while chunk := child.stdout.read1(65536):
    if size + len(chunk) > LIMIT:
        # Keep the newest output: rotate once instead of growing without bound.
        log.close()
        os.replace(log_path, log_path + ".1")
        log = open(log_path, "ab", buffering=0)
        size = 0
    log.write(chunk)
    size += len(chunk)
code = child.wait()
log.close()
record("status", "exited " + str(code))
