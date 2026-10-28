"""The line trace bash reads before a build or compose setup script: it reports each line further
down that the script reaches, never a line of a function, a subshell or another shell, and it
changes nothing about how the script runs, fails or traces itself."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

HELPERS = Path(__file__).resolve().parents[1] / "helpers"
TRACE = HELPERS / "script_trace.sh"
RUNNER = HELPERS / "setup_runner.py"
MARKER = re.compile(rb"^THORIC_LINE=(\d+)$", re.MULTILINE)


def build(script):
    """Runs `script` the way a build's RUN step does."""
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "thoric-setup.sh"
        path.write_bytes(script.encode())
        return subprocess.run(
            ["bash", str(path)],
            env={**os.environ, "BASH_ENV": str(TRACE)},
            capture_output=True,
            timeout=60,
        )


def lines(output):
    return [int(line) for line in MARKER.findall(output)]


class TraceTest(unittest.TestCase):
    def test_lines_only_move_forward_and_count_only_the_scripts_own_commands(self):
        result = build(
            "echo one\n"  # 1
            "helper() {\n"  # 2
            "  echo inside\n"  # 3
            "}\n"  # 4
            "for i in 1 2 3; do\n"  # 5
            "  echo $i\n"  # 6
            "done\n"  # 7
            "helper\n"  # 8
            "( echo sub )\n"  # 9
            "echo last\n"  # 10
        )
        self.assertEqual(result.returncode, 0, result)
        found = lines(result.stderr)
        self.assertEqual(found, sorted(set(found)))
        self.assertIn(1, found)
        self.assertIn(10, found)
        self.assertNotIn(3, found)
        self.assertNotIn(b"THORIC_LINE", result.stdout)

    def test_another_shell_the_script_starts_reports_nothing(self):
        # The child's echo is on its line 40, past every line of the script.
        result = build("bash -c \"$(printf '\\n%.0s' {1..39}; echo 'echo child')\"\necho done\n")
        self.assertEqual(result.returncode, 0, result)
        self.assertEqual(result.stdout, b"child\ndone\n")
        self.assertEqual(lines(result.stderr), [1, 2])

    def test_xtrace_errexit_and_the_exit_status_are_the_scripts_own(self):
        result = build("set -euxo pipefail\necho traced\nfalse\necho never\n")
        self.assertEqual(result.returncode, 1, result)
        self.assertNotIn(b"never", result.stdout)
        self.assertIn(b"+ echo traced", result.stderr)
        for leaked in (b"__thoric", b"LINENO", b"printf"):
            self.assertNotIn(leaked, result.stderr)
        self.assertEqual(build("exit 17\n").returncode, 17)

    def test_lines_still_arrive_once_the_script_drops_its_stderr(self):
        result = build("exec 2>/dev/null\necho quiet >&2\necho done\n")
        self.assertEqual(result.returncode, 0, result)
        self.assertEqual(lines(result.stderr), [1, 2, 3])
        self.assertNotIn(b"quiet", result.stderr)

    def test_a_debug_trap_of_the_script_ends_the_trace_without_failing_it(self):
        result = build("echo one\ntrap 'true' DEBUG\necho two\necho three\n")
        self.assertEqual((result.returncode, result.stdout), (0, b"one\ntwo\nthree\n"))
        self.assertEqual(lines(result.stderr), [1, 2])


class RunnerTraceTest(unittest.TestCase):
    def run_setup(self, request):
        return subprocess.run(
            [sys.executable, "-c", RUNNER.read_text()],
            input=json.dumps({"parameters": {}, "secrets": [], "mask": [], "timeout_seconds": 30, **request}).encode(),
            capture_output=True,
            timeout=60,
        )

    def test_the_runner_reads_the_trace_before_the_script(self):
        result = self.run_setup({"script": 'echo "${BASH_ENV-unset}"\nfalse\necho never\n', "trace": TRACE.read_text()})
        self.assertEqual(result.returncode, 1, result)
        self.assertEqual(result.stdout, b"THORIC_LINE=1\nunset\nTHORIC_LINE=2\n")

    def test_without_a_trace_the_script_runs_as_before(self):
        result = self.run_setup({"script": 'echo "${BASH_ENV-unset}"\n'})
        self.assertEqual((result.returncode, result.stdout), (0, b"unset\n"))


if __name__ == "__main__":
    unittest.main()
