"""Read, watch and change the MCP servers a desktop declares, as the desktop user.

The image declares servers in /etc/thoric/mcp.d and the desktop user in
~/.config/thoric/mcp, one JSON file per server named after it. Symlinks are never followed.
stdout carries one JSON document per line and nothing else.

While it watches, the manager sends the state of the servers and the endpoints that reach this
desktop on stdin, one JSON document per line, and the helper keeps the latest in the desktop's
runtime folder for thoric-mcp and its tray, until stdin closes.
"""
import json
import os
import re
import select
import stat
import sys
import time

IMAGE = "/etc/thoric/mcp.d"
CONTAINER = "/home/desktop/.config/thoric/mcp"
STATUS = "/run/desktop/thoric"
STATUS_FILE = "mcp-status.json"
NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,31}$")
FILE_LIMIT = 64 * 1024
SERVER_LIMIT = 64
# Tool lists of every server with their descriptions, as the manager trims them.
STATUS_LIMIT = 16 << 20


def open_folder(path, create=False):
    """Open a folder by walking it from / without following a symlink, creating it when asked."""
    folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.strip("/").split("/"):
            try:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
            except FileNotFoundError:
                if not create:
                    raise
                os.mkdir(part, 0o700, dir_fd=folder)
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
            os.close(folder)
            folder = child
        return folder
    except BaseException:
        os.close(folder)
        raise


def read_folder(path, source):
    try:
        folder = open_folder(path)
    except (FileNotFoundError, NotADirectoryError):
        return []
    except OSError as error:
        return [{"name": "", "source": source, "error": "Cannot open " + path + ": " + error.strerror}]
    servers = []
    try:
        for entry in sorted(os.listdir(folder)):
            if not entry.endswith(".json") or entry.startswith("."):
                continue
            name = entry[:-5]
            server = {"name": name, "source": source}
            servers.append(server)
            if len(servers) > SERVER_LIMIT:
                servers.pop()
                break
            if not NAME.match(name):
                server["error"] = "A server's file name must be lowercase letters, digits and dashes, at most 32, with .json"
                continue
            try:
                descriptor = os.open(entry, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=folder)
            except OSError as error:
                server["error"] = "Cannot read " + entry + ": " + (error.strerror or "error")
                continue
            with os.fdopen(descriptor, "rb") as file:
                info = os.fstat(file.fileno())
                if not stat.S_ISREG(info.st_mode):
                    server["error"] = entry + " is not a regular file"
                    continue
                data = file.read(FILE_LIMIT + 1)
            if len(data) > FILE_LIMIT:
                server["error"] = entry + " is larger than 64 KiB"
                continue
            try:
                spec = json.loads(data)
            except (ValueError, UnicodeDecodeError):
                server["error"] = entry + " is not valid JSON"
                continue
            if not isinstance(spec, dict):
                server["error"] = entry + " must hold a JSON object"
                continue
            server["spec"] = spec
    finally:
        os.close(folder)
    return servers


def listing():
    return {"servers": read_folder(IMAGE, "image") + read_folder(CONTAINER, "container")}


def emit(document):
    sys.stdout.write(json.dumps(document, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def signature(path):
    """What changes when a server file is added, removed or rewritten."""
    try:
        folder = open_folder(path)
    except OSError:
        return None
    try:
        entries = []
        for entry in sorted(os.listdir(folder)):
            try:
                info = os.stat(entry, dir_fd=folder, follow_symlinks=False)
            except OSError:
                continue
            entries.append((entry, info.st_mtime_ns, info.st_size, info.st_ino))
        return entries
    finally:
        os.close(folder)


def write_status(status):
    """Replace the status file atomically, and return the inode it has now."""
    data = (json.dumps(status, separators=(",", ":")) + "\n").encode("utf-8")
    folder = open_folder(STATUS, create=True)
    try:
        temporary = ".%s.%d.tmp" % (STATUS_FILE, os.getpid())
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600,
                             dir_fd=folder)
        try:
            with os.fdopen(descriptor, "wb") as file:
                file.write(data)
                inode = os.fstat(file.fileno()).st_ino
            os.rename(temporary, STATUS_FILE, src_dir_fd=folder, dst_dir_fd=folder)
        except BaseException:
            try:
                os.unlink(temporary, dir_fd=folder)
            except OSError:
                pass
            raise
        return inode
    finally:
        os.close(folder)


def clear_status(inode):
    """Remove the status file if it is still the one this helper wrote."""
    try:
        folder = open_folder(STATUS)
    except OSError:
        return
    try:
        if os.stat(STATUS_FILE, dir_fd=folder, follow_symlinks=False).st_ino == inode:
            os.unlink(STATUS_FILE, dir_fd=folder)
    except OSError:
        pass
    finally:
        os.close(folder)


def received(line):
    """The status a line from the manager carries, or None."""
    try:
        document = json.loads(line)
    except (ValueError, UnicodeDecodeError):
        return None
    status = document.get("status") if isinstance(document, dict) else None
    return status if isinstance(status, dict) else None


def watch():
    """Print the listing now and whenever it changes, and keep the status the manager sends, until
    stdin closes."""
    last = None
    descriptor = sys.stdin.fileno()
    buffer = bytearray()
    skipping = False
    inode = None
    try:
        while True:
            current = (signature(IMAGE), signature(CONTAINER))
            if current != last:
                emit(listing())
                last = current
            readable, _, _ = select.select([descriptor], [], [], 2)
            if not readable:
                continue
            chunk = os.read(descriptor, 1 << 16)
            if not chunk:
                return
            start = 0
            while True:
                end = chunk.find(b"\n", start)
                if end < 0:
                    if not skipping:
                        buffer += chunk[start:]
                        # A line longer than the limit is dropped up to its end.
                        if len(buffer) > STATUS_LIMIT:
                            buffer.clear()
                            skipping = True
                    break
                if not skipping:
                    buffer += chunk[start:end]
                    status = received(bytes(buffer))
                    if status is not None:
                        try:
                            inode = write_status(status)
                        except OSError:
                            pass
                buffer.clear()
                skipping = False
                start = end + 1
    finally:
        if inode is not None:
            clear_status(inode)


def write(name):
    if not NAME.match(name):
        raise ValueError("Invalid server name")
    data = sys.stdin.buffer.read(FILE_LIMIT + 1)
    if len(data) > FILE_LIMIT:
        raise ValueError("A server declaration must be at most 64 KiB")
    json.loads(data)
    folder = open_folder(CONTAINER, create=True)
    try:
        temporary = ".%s.%d.tmp" % (name, os.getpid())
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600,
                             dir_fd=folder)
        try:
            with os.fdopen(descriptor, "wb") as file:
                file.write(data)
                file.flush()
                os.fsync(file.fileno())
            os.rename(temporary, name + ".json", src_dir_fd=folder, dst_dir_fd=folder)
        except BaseException:
            try:
                os.unlink(temporary, dir_fd=folder)
            except OSError:
                pass
            raise
    finally:
        os.close(folder)
    emit({"written": name})


def remove(name):
    if not NAME.match(name):
        raise ValueError("Invalid server name")
    folder = open_folder(CONTAINER)
    try:
        os.unlink(name + ".json", dir_fd=folder)
    finally:
        os.close(folder)
    emit({"removed": name})


def main(arguments):
    command = arguments[0] if arguments else ""
    if command == "list":
        emit(listing())
    elif command == "watch":
        watch()
    elif command == "write" and len(arguments) == 2:
        write(arguments[1])
    elif command == "remove" and len(arguments) == 2:
        remove(arguments[1])
    else:
        raise ValueError("Usage: list | watch | write NAME | remove NAME")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except BrokenPipeError:
        sys.exit(0)
    except FileNotFoundError:
        sys.stderr.write("No such server\n")
        sys.exit(2)
    except (OSError, ValueError) as error:
        sys.stderr.write(str(error) + "\n")
        sys.exit(1)
