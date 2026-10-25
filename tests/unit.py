"""Runs every Python unit test of the repository: the test_*.py files in this folder and in the tests
folder of each project under core/, images/ and packaging/. Each folder is discovered on its own, so
a project's tests stay beside it without making the folders above them packages.

    python3 tests/unit.py [-v]
"""

from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
FOLDERS = [ROOT / "tests", *sorted(path for group in ("core", "images", "packaging") for path in ROOT.glob(f"{group}/*/tests"))]


def main():
    suite = unittest.TestSuite()
    for folder in FOLDERS:
        if any(folder.glob("test_*.py")):
            suite.addTests(unittest.TestLoader().discover(str(folder), pattern="test_*.py", top_level_dir=str(folder)))
    verbosity = 2 if "-v" in sys.argv[1:] else 1
    result = unittest.TextTestRunner(verbosity=verbosity).run(suite)
    sys.exit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
