"""Stream one regular file to stdout as the desktop user without following symlinks."""
import os
import shutil
import stat
import sys

path, limit = sys.argv[1], int(sys.argv[2])
folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
try:
    parts = path.lstrip("/").split("/")
    for part in parts[:-1]:
        child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
        os.close(folder)
        folder = child
    descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=folder)
    with os.fdopen(descriptor, "rb") as handle:
        info = os.fstat(handle.fileno())
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("Only regular files can be downloaded")
        if info.st_size > limit:
            raise ValueError("File is larger than the %d-byte limit" % limit)
        shutil.copyfileobj(handle, sys.stdout.buffer, 1024 * 1024)
        sys.stdout.buffer.flush()
except Exception as error:
    print("Download failed: " + str(error), file=sys.stderr)
    sys.exit(1)
finally:
    os.close(folder)
