#!/usr/bin/env python3
"""Register MCP servers of this desktop with Bilskirnir.

Bilskirnir starts a registered server as the desktop user when an agent needs it, and offers its
tools on the MCP endpoints that cover this container. The image's own servers are in
/etc/bilskirnir/mcp.d; this command manages the desktop user's, in ~/.config/bilskirnir/mcp.
"""
import argparse
import json
import os
import re
import sys

IMAGE = "/etc/bilskirnir/mcp.d"
FOLDER = "/home/desktop/.config/bilskirnir/mcp"
NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,31}$")
USAGE = """bilskirnir-mcp add NAME [--description TEXT] [--env KEY=VALUE]... [--cwd DIR] -- COMMAND [ARG]...
       bilskirnir-mcp add NAME --url http://127.0.0.1:PORT/mcp [--header 'Name: value']... [--description TEXT]
       bilskirnir-mcp remove NAME
       bilskirnir-mcp list"""


def folder(path, create):
    """Open a folder without following a symlink on the way, creating it when asked."""
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.strip("/").split("/"):
            try:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            except FileNotFoundError:
                if not create:
                    raise
                os.mkdir(part, 0o700, dir_fd=descriptor)
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def declaration(arguments):
    spec = {}
    if arguments.description:
        spec["description"] = arguments.description
    if arguments.url:
        if arguments.command or arguments.env or arguments.cwd:
            raise ValueError("--url is for an HTTP server; a command takes --env and --cwd")
        spec["url"] = arguments.url
        headers = {}
        for header in arguments.header:
            name, separator, value = header.partition(":")
            if not separator or not name.strip():
                raise ValueError("A header is 'Name: value': " + header)
            headers[name.strip()] = value.strip()
        if headers:
            spec["headers"] = headers
        return spec
    if arguments.header:
        raise ValueError("--header is for an HTTP server added with --url")
    if not arguments.command:
        raise ValueError("Give the server's command after --, or its --url")
    spec["command"] = arguments.command[0]
    if len(arguments.command) > 1:
        spec["args"] = arguments.command[1:]
    env = {}
    for variable in arguments.env:
        key, separator, value = variable.partition("=")
        if not separator or not key:
            raise ValueError("An environment variable is KEY=VALUE: " + variable)
        env[key] = value
    if env:
        spec["env"] = env
    if arguments.cwd:
        spec["cwd"] = arguments.cwd
    return spec


def add(arguments, target):
    if not NAME.match(arguments.name):
        raise ValueError("A server name is lowercase letters, digits and dashes, at most 32")
    data = (json.dumps(declaration(arguments), indent=2) + "\n").encode("utf-8")
    directory = folder(target, create=True)
    try:
        temporary = ".%s.%d.tmp" % (arguments.name, os.getpid())
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600,
                             dir_fd=directory)
        try:
            with os.fdopen(descriptor, "wb") as file:
                file.write(data)
            os.rename(temporary, arguments.name + ".json", src_dir_fd=directory, dst_dir_fd=directory)
        except BaseException:
            try:
                os.unlink(temporary, dir_fd=directory)
            except OSError:
                pass
            raise
    finally:
        os.close(directory)
    print("Registered %s. Bilskirnir reads its tools within a few seconds." % arguments.name)


def remove(arguments, target):
    if not NAME.match(arguments.name):
        raise ValueError("A server name is lowercase letters, digits and dashes, at most 32")
    directory = folder(target, create=False)
    try:
        os.unlink(arguments.name + ".json", dir_fd=directory)
    finally:
        os.close(directory)
    print("Removed %s." % arguments.name)


def listing(image, target):
    rows = []
    for source, path in (("image", image), ("container", target)):
        try:
            directory = folder(path, create=False)
        except OSError:
            continue
        try:
            for entry in sorted(os.listdir(directory)):
                if not entry.endswith(".json") or entry.startswith("."):
                    continue
                try:
                    descriptor = os.open(entry, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory)
                    with os.fdopen(descriptor, "rb") as file:
                        spec = json.loads(file.read(65537))
                    shown = spec.get("url") or " ".join([spec.get("command", "")] + spec.get("args", []))
                except (OSError, ValueError, AttributeError, TypeError):
                    shown = "(unreadable)"
                rows.append((entry[:-5], source, shown))
        finally:
            os.close(directory)
    for name, source, shown in rows:
        print("%-24s %-10s %s" % (name, source, shown))
    if not rows:
        print("No MCP servers are registered in this desktop.")


def main(argv, image=IMAGE, target=FOLDER, uid=None):
    parser = argparse.ArgumentParser(prog="bilskirnir-mcp", usage=USAGE, description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    adding = commands.add_parser("add")
    adding.add_argument("name")
    adding.add_argument("--description", default="")
    adding.add_argument("--env", action="append", default=[])
    adding.add_argument("--cwd")
    adding.add_argument("--url")
    adding.add_argument("--header", action="append", default=[])
    removing = commands.add_parser("remove")
    removing.add_argument("name")
    commands.add_parser("list")
    # Everything after -- is the server's command line, whatever options it has.
    program = []
    if "--" in argv:
        at = argv.index("--")
        argv, program = argv[:at], argv[at + 1:]
    arguments = parser.parse_args(argv)
    arguments.command = program
    if (os.geteuid() if uid is None else uid) == 0 and arguments.action != "list":
        raise ValueError("Run bilskirnir-mcp as the desktop user; the servers run as that user")
    if arguments.action == "add":
        add(arguments, target)
    elif arguments.action == "remove":
        remove(arguments, target)
    else:
        listing(image, target)


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except FileNotFoundError:
        sys.stderr.write("bilskirnir-mcp: no such server\n")
        sys.exit(1)
    except (OSError, ValueError) as error:
        sys.stderr.write("bilskirnir-mcp: %s\n" % error)
        sys.exit(1)
