"""Read a bounded slice of a regular file without following symlinks."""
import base64
import json
import os
import stat
import sys

path, offset, limit = sys.argv[1:]
folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
try:
    parts = path.lstrip("/").split("/")
    for part in parts[:-1]:
        child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
        os.close(folder)
        folder = child
    descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=folder)
    with os.fdopen(descriptor, "rb") as file:
        info = os.fstat(file.fileno())
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("Only regular files can be read")
        file.seek(int(offset))
        data = file.read(int(limit))
        print(json.dumps({"data_base64": base64.b64encode(data).decode("ascii"),
                          "bytes": len(data), "size": info.st_size,
                          "next_offset": int(offset) + len(data),
                          "eof": int(offset) + len(data) >= info.st_size}))
except Exception as error:
    print("Read failed: " + str(error), file=sys.stderr)
    sys.exit(1)
finally:
    os.close(folder)
