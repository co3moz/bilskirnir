"""Unpack a tar from standard input into a new folder of a volume as the desktop user, or remove
what an unpack that failed left there.

`unpack MOUNT PATH [TAR OPTION...]` creates PATH, a folder inside the volume mounted at MOUNT, in a
parent that is opened without following symlinks, and runs tar inside it with the options given.
The folder must not exist yet, so no file of the volume can be replaced and no symlink already in
the volume can lead the archive elsewhere. PATH `/` is the volume itself, accepted only while it
is empty. `ready` on standard output tells the manager that the folder is there and tar reads.

`clear MOUNT PATH` removes the folder again, or empties the volume for PATH `/`, without
following symlinks.
"""
import errno
import os
import stat
import sys


def open_dir(path):
    """Open a directory by walking each component with O_NOFOLLOW."""
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    for part in filter(None, path.split("/")):
        try:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
        finally:
            os.close(descriptor)
        descriptor = child
    return descriptor


def open_parent(mount, parent):
    try:
        return open_dir(mount + parent)
    except FileNotFoundError:
        raise ValueError(f"{parent or '/'} does not exist; create it first") from None
    except OSError as error:
        if error.errno in (errno.ENOTDIR, errno.ELOOP):
            raise ValueError(f"{parent} is not a folder; symlinks are not followed") from None
        raise


def conflict(message):
    print("CONFLICT: " + message, file=sys.stderr)
    sys.exit(17)


def unpack(mount, path, options):
    if path == "/":
        target = open_dir(mount)
        if os.listdir(target):
            conflict("The volume is not empty; unpack the archive into a new folder")
    else:
        parent, name = path.rsplit("/", 1)
        descriptor = open_parent(mount, parent)
        try:
            try:
                os.mkdir(name, 0o755, dir_fd=descriptor)
            except FileExistsError:
                conflict(f"{path} already exists; unpack the archive into a new folder")
            target = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
        finally:
            os.close(descriptor)
    # tar works in the folder created here, wherever its name may lead by now.
    os.fchdir(target)
    os.close(target)
    print("ready", flush=True)
    os.execvp("tar", ["tar", "--extract", "--file", "-", "--no-same-owner", *options])


def remove_tree(descriptor):
    for name in os.listdir(descriptor):
        info = os.lstat(name, dir_fd=descriptor)
        if stat.S_ISDIR(info.st_mode):
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            try:
                remove_tree(child)
            finally:
                os.close(child)
            os.rmdir(name, dir_fd=descriptor)
        else:
            os.unlink(name, dir_fd=descriptor)


def clear(mount, path):
    if path == "/":
        descriptor = open_dir(mount)
        try:
            remove_tree(descriptor)
        finally:
            os.close(descriptor)
        return
    parent, name = path.rsplit("/", 1)
    descriptor = open_dir(mount + parent)
    try:
        try:
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
        except FileNotFoundError:
            return
        try:
            remove_tree(child)
        finally:
            os.close(child)
        os.rmdir(name, dir_fd=descriptor)
    finally:
        os.close(descriptor)


if __name__ == "__main__":
    command, mount, path, *options = sys.argv[1:]
    try:
        if command == "unpack":
            unpack(mount, path, options)
        elif command == "clear" and not options:
            clear(mount, path)
        else:
            sys.exit(2)
    except SystemExit:
        raise
    except Exception as error:
        print("Unpacking failed: " + str(error), file=sys.stderr)
        sys.exit(1)
