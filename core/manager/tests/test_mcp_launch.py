"""The launcher of a desktop MCP server that declares variables: they arrive on stdin, sized on
the command line, so their values are on no command line and in no environment of the manager's;
exactly those bytes are read, so the server reads the rest of stdin as the manager wrote it; and
variables that do not arrive whole, or a program that does not exist, start nothing."""

import json
from pathlib import Path
import subprocess
import sys
import unittest

LAUNCHER = Path(__file__).resolve().parents[1] / "helpers/mcp_launch.py"
SERVER = "import os, sys; print(os.environ['TOKEN']); print(sys.stdin.read(), end='')"


def launch(variables, rest=b"", size=None, program=(sys.executable, "-c", SERVER)):
    data = json.dumps(variables).encode()
    size = len(data) if size is None else size
    return subprocess.run([sys.executable, "-I", str(LAUNCHER), str(size), *program],
                          input=data + rest, capture_output=True, timeout=30)


@unittest.skipIf(sys.platform == "win32", "the launcher replaces itself with exec, as in a desktop")
class LaunchTest(unittest.TestCase):
    def test_the_server_gets_the_variables_and_the_rest_of_stdin(self):
        rest = b'{"jsonrpc":"2.0","method":"initialize"}\n' * 2000
        result = launch({"TOKEN": "s3crét=x y"}, rest)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "s3crét=x y\n".encode() + rest)

    def test_variables_that_do_not_arrive_whole_start_nothing(self):
        for variables, size in [({"TOKEN": "x"}, 1000), ({"TOKEN": "x"}, 5), ({"TOKEN": "x"}, 1 << 21),
                                ({"TOKEN": 1}, None), (["TOKEN"], None), ({"A=B": "x"}, None)]:
            result = launch(variables, size=size)
            self.assertEqual(result.returncode, 2, (variables, size))
            self.assertTrue(result.stderr.startswith(b"MCP launch: "), result.stderr)
            self.assertEqual(result.stdout, b"")

    def test_a_missing_program_says_so(self):
        result = launch({"TOKEN": "x"}, program=("thoric-no-such-server",))
        self.assertEqual(result.returncode, 127)
        self.assertIn(b"cannot run thoric-no-such-server", result.stderr)


if __name__ == "__main__":
    unittest.main()
