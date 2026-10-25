"""Write a folder to stdout as a gzip-compressed tar, without following symlinks on its path.

The folder's parent is opened one component at a time without following symlinks, and tar runs
there on the folder's name, so the archive holds the folder under its own name and keeps any
symlink inside it as a link. The caller attaches stdin and keeps it open: when it closes, as it
does when the caller stops reading, tar and its gzip are stopped instead of running on unread."""
import os
import signal
import stat
import subprocess
import sys
import threading

path = sys.argv[1]
parent, name = path.rsplit("/", 1)
folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
try:
    for part in filter(None, parent.split("/")):
        child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
        os.close(folder)
        folder = child
    if not stat.S_ISDIR(os.lstat(name, dir_fd=folder).st_mode):
        raise ValueError("Only folders download as an archive")
    os.fchdir(folder)
except Exception as error:
    print("Archive failed: " + str(error), file=sys.stderr)
    sys.exit(2)
finally:
    os.close(folder)

# A session of its own, so stopping it reaches the gzip that tar starts too.
tar = subprocess.Popen(["tar", "--create", "--gzip", "--file", "-", "--", name],
                       stdin=subprocess.DEVNULL, start_new_session=True)


def stop_when_stdin_ends():
    # The descriptor itself: a buffered read would hold a lock the interpreter's exit waits for.
    while os.read(0, 65536):
        pass
    if tar.poll() is None:
        try:
            os.killpg(tar.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass


threading.Thread(target=stop_when_stdin_ends, daemon=True).start()
status = tar.wait()
# A stopped tar ends as a shell reports a signal; the watching thread is left behind.
os._exit(status if status >= 0 else 128 - status)
