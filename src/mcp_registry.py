"""Read, watch and change the MCP servers a desktop declares, as the desktop user.

The image declares servers in /etc/bilskirnir/mcp.d and the desktop user in
~/.config/bilskirnir/mcp, one JSON file per server named after it. Symlinks are never followed.
stdout carries one JSON document per line and nothing else.
"""
import json
import os
import re
import select
import stat
import sys
import time

IMAGE = "/etc/bilskirnir/mcp.d"
CONTAINER = "/home/desktop/.config/bilskirnir/mcp"
NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,31}$")
FILE_LIMIT = 64 * 1024
SERVER_LIMIT = 64


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


def watch():
    """Print the listing now and whenever it changes, until stdin closes."""
    last = None
    while True:
        current = (signature(IMAGE), signature(CONTAINER))
        if current != last:
            emit(listing())
            last = current
        readable, _, _ = select.select([sys.stdin], [], [], 2)
        if readable and not sys.stdin.buffer.read1(4096):
            return


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
