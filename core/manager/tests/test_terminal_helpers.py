"""The terminal runner and its control: statuses of several terminals in one call, a terminal whose
files are gone marked so, a closed terminal that always ends, even a shell that ignores the
hangup, and leaves no files behind, and terminals kept in a folder of the user's own that nothing
another user prepared, such as a link, takes the place of."""

import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HELPERS = Path(__file__).resolve().parents[1] / "helpers"
RUNNER = HELPERS / "terminal_runner.py"
CONTROL = HELPERS / "terminal_control.py"


def control(*arguments):
    result = subprocess.run([sys.executable, str(CONTROL), *arguments], capture_output=True, text=True, timeout=30)
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    return json.loads(result.stdout)


class TerminalTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root, True)
        self.parent = self.root / "terminals"

    def open(self, name):
        """Starts a runner as `docker exec --detach` would and waits until its shell runs."""
        directory = self.parent / name
        runner = subprocess.Popen([sys.executable, str(RUNNER), str(directory), str(self.root)])
        self.addCleanup(runner.wait)
        self.addCleanup(runner.kill)
        deadline = time.monotonic() + 10
        while control("status", str(directory))[0]["state"] != "running":
            self.assertLess(time.monotonic(), deadline, "the terminal did not start")
            time.sleep(0.1)
        return directory, runner

    def type(self, directory, text):
        control("write", str(directory), base64.b64encode(text.encode()).decode())

    def test_one_call_reports_every_terminal_in_order(self):
        first, _ = self.open("first")
        second, _ = self.open("second")
        missing = self.parent / "missing"
        statuses = control("status", str(first), str(missing), str(second))
        self.assertEqual([status["state"] for status in statuses], ["running", "unknown", "running"])
        # A terminal whose files went with a desktop restart is gone for good; the others are not.
        self.assertTrue(statuses[1]["gone"])
        self.assertNotIn("gone", statuses[0])

    def test_closing_ends_the_shell_and_removes_its_files(self):
        directory, runner = self.open("closed")
        self.type(directory, "echo hello\n")
        deadline = time.monotonic() + 10
        while "hello" not in control("read", str(directory), "0", "65536", "true")["output"]:
            self.assertLess(time.monotonic(), deadline, "the shell did not answer")
            time.sleep(0.1)
        status = control("stop", str(directory), "10")
        self.assertEqual(status["state"], "exited")
        self.assertFalse(directory.exists())
        self.assertEqual(runner.wait(10), 0)

    def test_a_shell_that_ignores_the_hangup_is_killed(self):
        directory, runner = self.open("stubborn")
        self.type(directory, "trap '' HUP\n")
        time.sleep(1)
        started = time.monotonic()
        status = control("stop", str(directory), "10")
        self.assertEqual(status["state"], "exited")
        self.assertEqual(status["signal"], 9)
        self.assertLess(time.monotonic() - started, 9)
        self.assertFalse(directory.exists())
        self.assertEqual(runner.wait(10), 0)


class FolderTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root, True)
        self.elsewhere = self.root / "elsewhere"
        self.elsewhere.mkdir()

    def test_a_runner_refuses_terminals_folder_that_is_a_link(self):
        parent = self.root / "terminals"
        parent.symlink_to(self.elsewhere)
        result = subprocess.run([sys.executable, str(RUNNER), str(parent / "t1"), str(self.root)],
                                capture_output=True, text=True, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(self.elsewhere.iterdir()), [])
        # Its control counts no terminal there, and stopping one makes nothing through the link.
        (self.elsewhere / "t1").mkdir()
        self.assertTrue(control("status", str(parent / "t1"))[0]["gone"])
        control("stop", str(parent / "t1"), "0")
        self.assertEqual(list((self.elsewhere / "t1").iterdir()), [])

    def test_the_terminals_folder_is_made_private(self):
        parent = self.root / "terminals"
        parent.mkdir(mode=0o755)
        os.chmod(parent, 0o755)
        runner = subprocess.Popen([sys.executable, str(RUNNER), str(parent / "t1"), str(self.root)])
        self.addCleanup(runner.wait)
        self.addCleanup(runner.kill)
        deadline = time.monotonic() + 10
        while control("status", str(parent / "t1"))[0]["state"] != "running":
            self.assertLess(time.monotonic(), deadline, "the terminal did not start")
            time.sleep(0.1)
        self.assertEqual(parent.stat().st_mode & 0o777, 0o700)
        # A terminal folder that exists already is not taken over.
        second = subprocess.run([sys.executable, str(RUNNER), str(parent / "t1"), str(self.root)],
                                capture_output=True, text=True, timeout=30)
        self.assertNotEqual(second.returncode, 0)

    def test_closing_follows_no_link_left_in_a_terminals_folder(self):
        parent = self.root / "terminals"
        parent.mkdir(mode=0o700)
        directory = parent / "t1"
        directory.mkdir()
        victim = self.elsewhere / "victim"
        victim.write_text("kept")
        (directory / "stop").symlink_to(victim)
        result = subprocess.run([sys.executable, str(CONTROL), "stop", str(directory), "0"],
                                capture_output=True, text=True, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(victim.read_text(), "kept")


if __name__ == "__main__":
    unittest.main()
