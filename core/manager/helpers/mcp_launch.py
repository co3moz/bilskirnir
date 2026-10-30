"""Start a desktop MCP server with the variables its declaration names, as the desktop user.

The manager runs this with the size of a JSON object, the server's program and its arguments, and
writes that object first on stdin: the variables, whose values may hold a compose secret, so they
stay off every command line and out of the environment of the manager's Docker CLI, which a
variable such as DOCKER_HOST would otherwise redirect. Exactly those bytes are read, and the
program, which takes this process's place, reads the rest of stdin itself.
"""
import json
import os
import sys

LIMIT = 1 << 20


def read_exactly(size):
    data = b""
    while len(data) < size:
        chunk = os.read(0, size - len(data))
        if not chunk:
            raise ValueError("stdin ended before the server's variables")
        data += chunk
    return data


def read_variables(size):
    if not 0 <= size <= LIMIT:
        raise ValueError("the server's variables are larger than 1 MiB")
    variables = json.loads(read_exactly(size))
    if not isinstance(variables, dict) or not all(
        isinstance(key, str) and isinstance(value, str) and key and "=" not in key and "\0" not in key + value
        for key, value in variables.items()
    ):
        raise ValueError("the server's variables must map names to text")
    return variables


def main(arguments):
    if len(arguments) < 2:
        sys.stderr.write("MCP launch: give the size of the variables and the server's program\n")
        return 2
    try:
        variables = read_variables(int(arguments[0]))
    except ValueError as error:
        sys.stderr.write("MCP launch: " + str(error) + "\n")
        return 2
    os.environ.update(variables)
    program = arguments[1:]
    try:
        os.execvp(program[0], program)
    except OSError as error:
        sys.stderr.write("MCP launch: cannot run " + program[0] + ": " + (error.strerror or str(error)) + "\n")
        return 127


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
