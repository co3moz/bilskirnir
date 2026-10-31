"""The folders of the helpers that run as root in a desktop, under /tmp and /run where the desktop
user may make files first: the gamepad's and the desktop proxy's are never a link or a folder
another user made, their controls follow no link inside them, and a second gamepad daemon leaves the
first one's folder alone."""

import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HELPERS = Path(__file__).resolve().parents[1] / "helpers"


def run(helper, *arguments):
    return subprocess.run([sys.executable, str(HELPERS / helper), *arguments], capture_output=True, text=True,
                          stdin=subprocess.DEVNULL, timeout=30)


class FolderTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root, True)
        self.elsewhere = self.root / "elsewhere"
        self.elsewhere.mkdir()

    def test_the_gamepad_daemon_refuses_a_folder_that_is_a_link(self):
        folder = self.root / "gamepad"
        folder.symlink_to(self.elsewhere)
        result = run("gamepad_daemon.py", str(folder))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(self.elsewhere.iterdir()), [])

    def test_a_second_gamepad_daemon_leaves_the_first_ones_folder_alone(self):
        folder = self.root / "gamepad"
        folder.mkdir(mode=0o700)
        # The test holds the lock as a running daemon does.
        with open(folder / "lock", "w") as first:
            fcntl.flock(first, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = run("gamepad_daemon.py", str(folder))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sorted(path.name for path in folder.iterdir()), ["lock"])

    def test_the_gamepad_daemon_takes_no_lock_through_a_link(self):
        folder = self.root / "gamepad"
        folder.mkdir(mode=0o700)
        victim = self.elsewhere / "victim"
        victim.write_text("kept")
        (folder / "lock").symlink_to(victim)
        self.assertNotEqual(run("gamepad_daemon.py", str(folder)).returncode, 0)
        self.assertEqual(victim.read_text(), "kept")
        self.assertFalse((folder / "status").exists())

    def test_the_gamepad_control_reads_and_writes_nothing_through_a_link(self):
        (self.elsewhere / "status").write_text(json.dumps({"state": "running", "pid": os.getpid()}))
        folder = self.root / "gamepad"
        folder.symlink_to(self.elsewhere)
        status = json.loads(run("gamepad_control.py", str(folder), "status").stdout)
        self.assertEqual(status["state"], "absent")
        run("gamepad_control.py", str(folder), "stop")
        self.assertFalse((self.elsewhere / "stop").exists())

    def test_the_gamepad_control_follows_no_link_inside_its_folder(self):
        folder = self.root / "gamepad"
        folder.mkdir(mode=0o700)
        victim = self.elsewhere / "victim"
        victim.write_text("kept")
        (folder / "stop").symlink_to(victim)
        self.assertNotEqual(run("gamepad_control.py", str(folder), "stop").returncode, 0)
        self.assertEqual(victim.read_text(), "kept")

    def test_the_proxy_folder_is_never_a_link_or_another_users(self):
        link = self.root / "proxy-link"
        link.symlink_to(self.elsewhere)
        result = run("outbound_control.py", str(link), "status")
        self.assertNotEqual(result.returncode, 0)
        # The test's own user stands in for the desktop user, who made the folder first.
        if os.geteuid() not in (0, 65534):
            mine = self.root / "proxy"
            mine.mkdir()
            result = run("outbound_control.py", str(mine), "status")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("belongs to another user", result.stderr)


if __name__ == "__main__":
    unittest.main()
