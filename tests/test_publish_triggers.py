"""Which changes run .github/workflows/publish.yml. A push to master publishes images, so it runs only
for what they are built from, and a change to the documentation alone publishes nothing. A pull
request runs the checks, so it also runs for every file a test reads: the Rust and interface tests
hold docs/API.md, docs/MCP.md and the README to the routes and tools they describe, and a change to
those alone could break them without any check running.
"""

import os
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
# The ways the tests name a file of the repository, relative to the file that names them: Rust's
# include_str! and include_bytes!, and a URL of an interface test's module.
READS = (
    ("core/manager", "*.rs", re.compile(r'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)')),
    ("core/web/tests", "*.ts", re.compile(r'new URL\(\s*"([^"]+)",\s*import\.meta\.url\s*\)')),
)


def trigger_paths(event):
    """The `paths` list of one event of the workflow, read from its lines, since the checks have no
    YAML library."""
    workflow = (ROOT / ".github/workflows/publish.yml").read_text(encoding="utf-8")
    block = re.search(rf"(?m)^  {event}:\n(?:    .*\n)*?    paths:\n((?:      - .*\n)+)", workflow)
    assert block, f"the {event} trigger has no written-out paths list"
    paths = re.findall(r"(?m)^      - '([^']+)'$", block.group(1))
    for path in paths:
        # GitHub reads ?, +, [ and ! too; matches() reads only * and **.
        assert not re.search(r"[?+\[\]!]", path), f"{path} uses a pattern character this test does not read"
    return paths


def matches(pattern, path):
    """Whether a GitHub filter pattern matches a path of the repository: `*` within a folder and `**`
    across folders."""
    regex = "".join(
        ".*" if part == "**" else "[^/]*" if part == "*" else re.escape(part)
        for part in re.split(r"(\*\*|\*)", pattern)
    )
    return re.fullmatch(regex, path) is not None


def files_tests_read():
    """Every file of the repository the Rust code and the interface tests read, from the root. An
    installed package is none."""
    found = set()
    for folder, suffix, form in READS:
        for source in (ROOT / folder).rglob(suffix):
            for name in form.findall(source.read_text(encoding="utf-8")):
                if "node_modules" not in Path(name).parts:
                    found.add(Path(os.path.normpath(source.parent / name)).relative_to(ROOT).as_posix())
    return found


class PublishTriggersTest(unittest.TestCase):
    def test_a_pull_request_runs_the_checks_for_every_file_a_test_reads(self):
        patterns = trigger_paths("pull_request")
        read = files_tests_read()
        self.assertLessEqual({"README.md", "docs/API.md", "docs/MCP.md"}, read)
        for path in sorted(read):
            with self.subTest(path=path):
                self.assertTrue((ROOT / path).is_file(), f"{path} is read by a test but is no file")
                self.assertTrue(
                    any(matches(pattern, path) for pattern in patterns),
                    f"a test reads {path}, so a pull request that changes only it must run the checks; "
                    "add it to the pull_request paths of .github/workflows/publish.yml",
                )

    def test_a_pull_request_runs_the_checks_for_everything_a_push_publishes(self):
        published = trigger_paths("push")
        self.assertEqual([path for path in published if path not in trigger_paths("pull_request")], [])

    def test_a_change_to_the_documentation_alone_publishes_nothing(self):
        published = trigger_paths("push")
        for path in ("README.md", "AGENTS.md", "docs/API.md"):
            with self.subTest(path=path):
                self.assertFalse(any(matches(pattern, path) for pattern in published), f"a push of {path} publishes")

    def test_the_patterns_match_as_github_does(self):
        self.assertTrue(matches("docs/**", "docs/screenshots/thoric.png"))
        self.assertTrue(matches("Cargo.*", "Cargo.lock"))
        self.assertTrue(matches(".github/workflows/publish.yml", ".github/workflows/publish.yml"))
        self.assertFalse(matches("Cargo.*", "Cargo-lock"))
        self.assertFalse(matches("compose*.yml", "deploy/compose.yml"))
        self.assertFalse(matches("core/**", "docs/core/API.md"))


if __name__ == "__main__":
    unittest.main()
