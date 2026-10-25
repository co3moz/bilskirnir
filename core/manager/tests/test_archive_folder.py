"""A folder's download: the archive holds the folder under its own name and keeps symlinks as
links, no symlink on the path is followed, and tar stops when the caller closes stdin."""

import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "helpers/archive_folder.py"


class ArchiveFolderTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)

    def archive(self, path):
        """Run the script with stdin held open until it ends, as the manager does."""
        with tempfile.TemporaryFile() as errors:
            process = subprocess.Popen([sys.executable, str(SCRIPT), str(path)],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
            output = process.stdout.read()
            process.wait(timeout=30)
            process.stdin.close()
            process.stdout.close()
            errors.seek(0)
            return process.returncode, output, errors.read()

    def test_the_archive_holds_the_folder_under_its_own_name(self):
        project = self.root / "my project"
        (project / "src").mkdir(parents=True)
        (project / "src/app.py").write_bytes(b"print(1)\n")
        (project / "README").write_bytes(b"hi")
        os.symlink("/etc/passwd", project / "link")
        code, output, errors = self.archive(project)
        self.assertEqual(code, 0, errors)
        with tarfile.open(fileobj=io.BytesIO(output), mode="r:gz") as tar:
            names = sorted(tar.getnames())
            self.assertEqual(names, ["my project", "my project/README", "my project/link",
                                     "my project/src", "my project/src/app.py"])
            self.assertEqual(tar.extractfile("my project/src/app.py").read(), b"print(1)\n")
            link = tar.getmember("my project/link")
            self.assertTrue(link.issym())
            self.assertEqual(link.linkname, "/etc/passwd")

    def test_a_name_like_an_option_stays_a_name(self):
        folder = self.root / "--remove-files"
        folder.mkdir()
        (folder / "kept").write_bytes(b"x")
        code, output, errors = self.archive(folder)
        self.assertEqual(code, 0, errors)
        self.assertTrue((folder / "kept").exists())
        with tarfile.open(fileobj=io.BytesIO(output), mode="r:gz") as tar:
            self.assertIn("--remove-files/kept", tar.getnames())

    def test_no_symlink_on_the_path_is_followed(self):
        (self.root / "real/folder").mkdir(parents=True)
        os.symlink(self.root / "real", self.root / "through")
        os.symlink(self.root / "real/folder", self.root / "folder-link")
        for path in [self.root / "through/folder", self.root / "folder-link"]:
            code, output, errors = self.archive(path)
            self.assertEqual(code, 2, path)
            self.assertEqual(output, b"")
            self.assertIn(b"Archive failed", errors)

    def test_only_folders_are_archived(self):
        (self.root / "file").write_bytes(b"x")
        code, _, errors = self.archive(self.root / "file")
        self.assertEqual(code, 2)
        self.assertIn(b"Only folders", errors)
        code, _, errors = self.archive(self.root / "missing")
        self.assertEqual(code, 2)
        self.assertIn(b"Archive failed", errors)

    def test_tar_stops_when_stdin_closes(self):
        folder = self.root / "large"
        folder.mkdir()
        # Incompressible, so the archive fills the pipe nobody reads and tar waits on it.
        (folder / "noise").write_bytes(os.urandom(4 << 20))
        process = subprocess.Popen([sys.executable, str(SCRIPT), str(folder)], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.addCleanup(process.stdout.close)
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass
        self.assertIsNone(process.returncode, "tar should be waiting on the full pipe")
        process.stdin.close()
        self.assertNotEqual(process.wait(timeout=10), 0)


if __name__ == "__main__":
    unittest.main()
