"""Unpacking an archive into a volume: only into a folder the helper creates, or into the volume
itself while it is empty, never through a symlink or over anything already there, and removed
again without following the symlinks an archive left."""

import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "src/volume_extract.py"


def archive(files, links=()):
    """A tar of these files, and symlinks as (name, target) pairs."""
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w", format=tarfile.GNU_FORMAT) as tar:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(content)
            info.mode = 0o664
            tar.addfile(info, io.BytesIO(content))
        for name, target in links:
            info = tarfile.TarInfo(name)
            info.type = tarfile.SYMTYPE
            info.linkname = target
            tar.addfile(info)
    return data.getvalue()


class ExtractTest(unittest.TestCase):
    def setUp(self):
        self.mount = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.mount)

    def run_script(self, *arguments, data=b""):
        return subprocess.run([sys.executable, str(SCRIPT), *arguments], input=data, capture_output=True)

    def unpack(self, path, data, *options):
        return self.run_script("unpack", str(self.mount), path, *options, data=data)

    def test_an_archive_lands_in_the_new_folder(self):
        (self.mount / "project").mkdir()
        result = self.unpack("/project/main", archive({"src/app.py": b"print(1)\n", "README": b"hi"}))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"ready\n")
        self.assertEqual((self.mount / "project/main/src/app.py").read_bytes(), b"print(1)\n")
        self.assertEqual((self.mount / "project/main/README").read_bytes(), b"hi")

    def test_tar_options_are_passed_on(self):
        result = self.unpack("/kept", archive({"mode": b"x"}), "--preserve-permissions")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.mount / "kept/mode").stat().st_mode & 0o777, 0o664)

    def test_nothing_that_exists_is_replaced(self):
        (self.mount / "project").mkdir()
        (self.mount / "project/keep.txt").write_bytes(b"mine")
        (self.mount / "elsewhere").mkdir()
        (self.mount / "link").symlink_to("elsewhere")
        for path in ["/project", "/link"]:
            result = self.unpack(path, archive({"keep.txt": b"theirs"}))
            self.assertEqual(result.returncode, 17, path)
            self.assertIn(b"CONFLICT: ", result.stderr)
            self.assertEqual(result.stdout, b"")
        self.assertEqual((self.mount / "project/keep.txt").read_bytes(), b"mine")
        self.assertEqual(list((self.mount / "elsewhere").iterdir()), [])

    def test_no_symlink_in_the_path_is_followed(self):
        (self.mount / "elsewhere").mkdir()
        (self.mount / "link").symlink_to("elsewhere")
        result = self.unpack("/link/new", archive({"file": b"x"}))
        self.assertEqual(result.returncode, 1)
        self.assertIn(b"symlinks are not followed", result.stderr)
        self.assertEqual(list((self.mount / "elsewhere").iterdir()), [])

    def test_the_folder_above_must_exist(self):
        result = self.unpack("/missing/new", archive({"file": b"x"}))
        self.assertEqual(result.returncode, 1)
        self.assertIn(b"/missing does not exist", result.stderr)
        self.assertFalse((self.mount / "missing").exists())

    def test_the_volume_itself_only_while_it_is_empty(self):
        result = self.unpack("/", archive({"file": b"x"}))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.mount / "file").read_bytes(), b"x")
        result = self.unpack("/", archive({"other": b"y"}))
        self.assertEqual(result.returncode, 17)
        self.assertFalse((self.mount / "other").exists())

    def test_an_archive_cannot_write_beside_its_folder(self):
        (self.mount / "beside").mkdir()
        result = self.unpack("/new", archive({"../beside/escaped": b"x", "/rooted": b"y"}))
        # tar refuses the member that climbs out and keeps a rooted one inside.
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list((self.mount / "beside").iterdir()), [])
        self.assertEqual((self.mount / "new/rooted").read_bytes(), b"y")

    def test_clearing_removes_the_folder_without_following_its_symlinks(self):
        (self.mount / "outside").mkdir()
        (self.mount / "outside/precious").write_bytes(b"keep")
        result = self.unpack("/new", archive({"file": b"x"}, links=[("out", "../outside")]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.mount / "new/out").is_symlink())
        result = self.run_script("clear", str(self.mount), "/new")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.mount / "new").exists())
        self.assertEqual((self.mount / "outside/precious").read_bytes(), b"keep")
        # A folder that is gone already has nothing to clear.
        self.assertEqual(self.run_script("clear", str(self.mount), "/new").returncode, 0)

    def test_clearing_the_volume_empties_it(self):
        self.assertEqual(self.unpack("/", archive({"a/b": b"x", "c": b"y"})).returncode, 0)
        result = self.run_script("clear", str(self.mount), "/")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.mount.is_dir())
        self.assertEqual(list(self.mount.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
