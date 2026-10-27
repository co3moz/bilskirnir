"""The compose setup runner: parameters reach the script as environment variables, secret values
never reach the output in one piece, and the script's exit status and time limit are kept."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
import unittest

RUNNER = Path(__file__).resolve().parents[1] / "helpers/setup_runner.py"
spec = importlib.util.spec_from_file_location("setup_runner", RUNNER)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def run(script, parameters=None, secrets=(), timeout_seconds=30, mask=()):
    request = {
        "script": script,
        "parameters": parameters or {},
        "secrets": list(secrets),
        "mask": list(mask),
        "timeout_seconds": timeout_seconds,
    }
    # The manager runs the helper with `python3 -c`, so it must work without its file name.
    return subprocess.run(
        [sys.executable, "-c", RUNNER.read_text()],
        input=json.dumps(request).encode(),
        capture_output=True,
        timeout=60,
    )


class MaskerTest(unittest.TestCase):
    def test_a_secret_split_across_reads_is_still_masked(self):
        masker = runner.Masker([b"hunter2"])
        output = b"".join(masker.push(bytes([b])) for b in b"pw=hunter2 done") + masker.finish()
        self.assertEqual(output, b"pw=*** done")

    def test_the_longest_secret_is_masked_first(self):
        masker = runner.Masker([b"abc", b"abcdef", b""])
        self.assertEqual(masker.push(b"abcdef abc") + masker.finish(), b"*** ***")

    def test_without_secrets_nothing_is_held_back(self):
        masker = runner.Masker([])
        self.assertEqual(masker.push(b"partial"), b"partial")


class RunnerTest(unittest.TestCase):
    def test_parameters_are_environment_variables_and_secrets_are_masked(self):
        result = run(
            'printf "%s|%s|%s\\n" "$ACCOUNT" "$PASSWORD" "$EMPTY"\nprintf "%s" "$PASSWORD" >&2\n',
            {"ACCOUNT": "dev@example.com", "PASSWORD": "s3cr3t-value", "EMPTY": ""},
            ["PASSWORD"],
        )
        self.assertEqual(result.returncode, 0, result)
        self.assertEqual(result.stdout, b"dev@example.com|***|\n***")
        self.assertNotIn(b"s3cr3t", result.stdout + result.stderr)

    def test_the_secrets_of_other_steps_are_masked_without_reaching_the_script(self):
        # An earlier step saved its token in a file; this step prints the file.
        result = run(
            'printf "%s|%s" "${TOKEN-unset}" "$(printf other-step-token)"',
            mask=["other-step-token", ""],
        )
        self.assertEqual(result.returncode, 0, result)
        self.assertEqual(result.stdout, b"unset|***")

    def test_a_multiline_secret_is_masked_whole(self):
        result = run('printf "%s" "$LICENSE"', {"LICENSE": "line one\nline two"}, ["LICENSE"])
        self.assertEqual(result.stdout, b"***")

    def test_the_script_fails_on_error_with_its_exit_status(self):
        result = run("false\necho never\n")
        self.assertEqual(result.returncode, 1)
        self.assertNotIn(b"never", result.stdout)
        self.assertEqual(run("exit 17\n").returncode, 17)

    def test_stdin_is_closed_so_the_script_is_not_consumed(self):
        result = run("cat\necho after-cat\n")
        self.assertEqual((result.returncode, result.stdout), (0, b"after-cat\n"))

    def test_the_script_file_is_gone_before_it_runs(self):
        result = run('ls "${TMPDIR:-/tmp}" | grep -c thoric-setup || true\n')
        self.assertEqual(result.stdout.strip(), b"0")

    def test_a_script_past_its_time_limit_is_stopped_with_its_children(self):
        started = time.monotonic()
        result = run("sleep 30 &\necho started\nwait\n", timeout_seconds=1)
        self.assertEqual(result.returncode, 124)
        self.assertIn(b"started", result.stdout)
        self.assertIn(b"did not finish within 1 seconds", result.stdout)
        self.assertLess(time.monotonic() - started, 15)

    def test_a_program_left_running_does_not_hold_the_job(self):
        started = time.monotonic()
        result = run("sleep 30 &\necho done\n")
        self.assertEqual((result.returncode, result.stdout), (0, b"done\n"))
        self.assertLess(time.monotonic() - started, 15)


if __name__ == "__main__":
    unittest.main()
