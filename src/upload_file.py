"""Receive one complete file as the desktop user; never follow path symlinks."""
import os
import signal
import sys
import uuid


def timed_out(*_):
    raise TimeoutError("File transfer timed out")


signal.signal(signal.SIGALRM, timed_out)
signal.alarm(3600)
directory, name, size, replace = sys.argv[1:]
expected = int(size)
folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
temporary = None
try:
    for part in directory.lstrip("/").split("/"):
        try:
            os.mkdir(part, 0o755, dir_fd=folder)
        except FileExistsError:
            pass
        child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
        os.close(folder)
        folder = child
    temporary = ".bilskirnir-upload-" + uuid.uuid4().hex
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644, dir_fd=folder)
    received = 0
    with os.fdopen(descriptor, "wb") as output:
        while chunk := sys.stdin.buffer.read(65536):
            received += len(chunk)
            if received > expected:
                raise ValueError("Received more bytes than expected")
            output.write(chunk)
        if received != expected:
            raise ValueError("File transfer was interrupted")
        output.flush()
        os.fsync(output.fileno())
    if replace == "true":
        os.replace(temporary, name, src_dir_fd=folder, dst_dir_fd=folder)
    else:
        try:
            os.link(temporary, name, src_dir_fd=folder, dst_dir_fd=folder, follow_symlinks=False)
        except FileExistsError:
            print("CONFLICT: File already exists. Select Replace existing files to overwrite it.", file=sys.stderr)
            sys.exit(17)
except Exception as error:
    print("Upload failed: " + str(error), file=sys.stderr)
    sys.exit(1)
finally:
    if temporary is not None:
        try:
            os.unlink(temporary, dir_fd=folder)
        except FileNotFoundError:
            pass
    os.close(folder)
