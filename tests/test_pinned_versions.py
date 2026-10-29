"""The versions Thoric is built with move only with a commit: every image a Dockerfile or a Compose
manifest starts from names an exact release or a dated build, which Dependabot proposes to raise,
and the manager image builds with the Rust and Bun releases the checks of
.github/workflows/publish.yml install.
"""

import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dependencies", ROOT / "core/manager/dependencies.py")
dependencies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dependencies)

# A release with three numbers, such as 2.11.7, a dated build, such as noble-20261024.1, or a digest.
EXACT = re.compile(r"^sha256:[0-9a-f]{64}$|(?<![\d.])(?:\d+\.\d+\.\d+|\d{8})(?!\d)")


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


class PinnedVersionsTest(unittest.TestCase):
    def test_every_image_names_an_exact_version(self):
        images = [item for item in dependencies.infrastructure_dependencies(ROOT) if item["kind"] == "image"]
        self.assertTrue(images)
        for item in images:
            with self.subTest(image=item["name"], used_in=item["used_in"]):
                self.assertRegex(
                    item["version"],
                    EXACT,
                    f"{item['name']}:{item['version']} moves under the build; name a release or a dated build",
                )

    def test_the_patterns_tell_moving_tags_from_exact_ones(self):
        for tag in ("1.99.1-bookworm", "28.5.2-cli", "bookworm-20261027-slim", "noble-20261024.1", "2.11.7-alpine"):
            self.assertRegex(tag, EXACT)
        for tag in ("1-bookworm", "28-cli", "bookworm-slim", "24.04", "2-alpine", "latest", "1.99-bookworm"):
            self.assertNotRegex(tag, EXACT)

    def test_the_manager_builds_with_the_rust_release_of_the_checks(self):
        channel = re.search(r'(?m)^channel = "([^"]+)"$', read("rust-toolchain.toml")).group(1)
        self.assertRegex(channel, r"^\d+\.\d+\.\d+$", "a channel such as stable moves with every Rust release")
        self.assertRegex(
            read("core/manager/Dockerfile"),
            rf"(?m)^FROM rust:{re.escape(channel)}-\w+ AS chef$",
            f"rust-toolchain.toml names Rust {channel}; core/manager/Dockerfile builds on another rust image",
        )
        # Without a toolchain name, rustup installs the one rust-toolchain.toml names.
        workflow = read(".github/workflows/publish.yml")
        self.assertRegex(workflow, r"(?m)^ +run: rustup toolchain install$")
        self.assertNotRegex(workflow, r"rustup (?:default|update|toolchain install [^\n])")

    def test_the_manager_runs_on_the_debian_release_it_was_built_on(self):
        dockerfile = read("core/manager/Dockerfile")
        built = re.search(r"(?m)^FROM rust:[\d.]+-(\w+) AS chef$", dockerfile).group(1)
        final = re.findall(r"(?m)^FROM (\S+)", dockerfile)[-1]
        self.assertRegex(final, rf"^debian:{built}-", f"the manager is linked on Debian {built} and must run on it")

    def test_the_interface_builds_with_the_bun_of_the_checks(self):
        image = re.search(r"(?m)^FROM oven/bun:(\d+\.\d+\.\d+)-slim AS ui$", read("core/manager/Dockerfile")).group(1)
        installed = set(re.findall(r"(?m)^ +bun-version: (\S+)$", read(".github/workflows/publish.yml")))
        self.assertEqual(
            installed,
            {image},
            f"core/manager/Dockerfile builds the interface with Bun {image}; raise every bun-version of "
            ".github/workflows/publish.yml with it",
        )


if __name__ == "__main__":
    unittest.main()
