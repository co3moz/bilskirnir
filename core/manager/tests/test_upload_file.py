"""Saving over a file: the new content keeps the replaced file's permission bits, a new file gets
the mode the caller names, and a refused replace leaves the old file alone."""

import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "helpers/upload_file.py"


class UploadFileTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)

    def write(self, name, data, replace, mode="600"):
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(self.root), name, str(len(data)), replace, mode],
            input=data, capture_output=True, timeout=30)

    def mode(self, name):
        return stat.S_IMODE((self.root / name).stat().st_mode)

    def test_a_saved_script_stays_executable(self):
        script = self.root / "run.sh"
        script.write_bytes(b"#!/bin/sh\necho old\n")
        os.chmod(script, 0o755)
        result = self.write("run.sh", b"#!/bin/sh\necho new\n", "true")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(script.read_bytes(), b"#!/bin/sh\necho new\n")
        self.assertEqual(self.mode("run.sh"), 0o755)

    def test_special_bits_of_the_replaced_file_are_not_carried_over(self):
        target = self.root / "tool"
        target.write_bytes(b"old")
        os.chmod(target, 0o4755)
        result = self.write("tool", b"new", "true")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.mode("tool"), 0o755)

    def test_a_new_file_gets_the_mode_it_is_written_with(self):
        result = self.write("notes.txt", b"hello", "true", "600")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.mode("notes.txt"), 0o600)

    def test_a_symlink_is_replaced_as_a_link_without_its_targets_mode(self):
        target = self.root / "target"
        target.write_bytes(b"target")
        os.chmod(target, 0o700)
        os.symlink("target", self.root / "link")
        result = self.write("link", b"new", "true", "644")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / "link").is_symlink())
        self.assertEqual(self.mode("link"), 0o644)
        self.assertEqual(target.read_bytes(), b"target")

    def test_without_replace_an_existing_file_is_kept(self):
        kept = self.root / "kept.txt"
        kept.write_bytes(b"kept")
        result = self.write("kept.txt", b"other", "false")
        self.assertEqual(result.returncode, 17)
        self.assertIn(b"CONFLICT", result.stderr)
        self.assertEqual(kept.read_bytes(), b"kept")
        self.assertEqual([path.name for path in self.root.iterdir()], ["kept.txt"])


if __name__ == "__main__":
    unittest.main()
