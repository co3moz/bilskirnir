"""The WSL package's `bilskirnir update`: which manager release it moves an installation to, which
desktop images it refreshes for that manager, how it brings the installation files of that release
without losing edits made by hand, and when it replaces the command itself with the one that
release bundles.

The script runs against a temporary installation directory with stand-ins for docker, curl,
sudo and systemctl on PATH, each recording its arguments. `docker run` serves the manager
image's files from a directory standing for its root filesystem.
"""

import hashlib
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "wsl/bilskirnir"
REPOSITORY = "ghcr.io/co3moz/bilskirnir"
FILES = ("compose.yml", "deploy/Caddyfile", ".env.example")
UNITS = ("bilskirnir-update.path", "bilskirnir-update.service")

STUB = """#!/usr/bin/env bash
printf '%s\\n' "$(basename "$0") $*" >> "$CALLS"
case "$(basename "$0") $*" in
    "curl "*) [[ -n ${RELEASE:-} ]] || exit 22; printf '{\\n  "url": "x",\\n  "tag_name": "%s",\\n  "name": "%s"\\n}\\n' "$RELEASE" "$RELEASE" ;;
    "docker pull "*) exit "${PULL_STATUS:-0}" ;;
    "docker run "*) exec cat "$IMAGE_ROOT${*: -1}" ;;
    "docker compose exec "*) printf '[{"reference": "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"}]\\n' ;;
    "docker compose ps "*) echo manager-container ;;
    "docker inspect "*) printf '%s\\n' "${RELEASE_LABEL-<no value>}" ;;
    "docker image inspect "*":24.04") [[ -n ${DOWNLOADED:-} ]] || exit 1 ;;
    "docker image inspect "*) [[ -n ${RELEASE_TAG_HERE:-} ]] || exit 1 ;;
    "sudo "*) exec "$@" ;;
esac
exit 0
"""


def shipped(files):
    """The checksum file the package writes with sha256sum."""
    return "".join(f"{hashlib.sha256(content.encode()).hexdigest()}  {name}\n" for name, content in files.items())


def copy_source(dockerfile, target):
    """The source of the COPY that puts a file at target, or None."""
    copy = re.search(rf"^COPY (?:--chmod=\d+ )?(\S+) {re.escape(target)}$", dockerfile, re.M)
    return copy and copy.group(1)


def package_level():
    """The level wsl/Dockerfile records for the package's own files, and where."""
    match = re.search(r"printf '(\d+)\\n' > (\S+)", (ROOT / "wsl/Dockerfile").read_text())
    return match and (int(match.group(1)), match.group(2))


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class UpdateTest(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        self.cli = self.dir / "bilskirnir"
        self.level = self.dir / "package-level"
        self.units = self.dir / "systemd"
        self.units.mkdir()
        self.script = CLI.read_text()
        for line, replacement in (
            ("INSTALL=/opt/bilskirnir", f"INSTALL={self.install}"),
            ("CLI=/usr/local/bin/bilskirnir", f"CLI={self.cli}"),
            ("PACKAGE_LEVEL=/usr/lib/wsl/package-level", f"PACKAGE_LEVEL={self.level}"),
            ("UNIT_DIR=/etc/systemd/system", f"UNIT_DIR={self.units}"),
        ):
            self.assertEqual(self.script.count(f"\n{line}\n"), 1, line)
            self.script = self.script.replace(f"\n{line}\n", f"\n{replacement}\n")
        # The command and the package level as the package of the installed release left them.
        self.cli.write_text(self.command("0.2.0"))
        self.cli.chmod(0o755)
        self.level.write_text(f"{package_level()[0]}\n")
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name in ("docker", "curl", "sudo", "systemctl"):
            (bin_dir / name).write_text(STUB)
            (bin_dir / name).chmod(0o755)
        self.calls = self.dir / "calls"
        self.image = self.dir / "image"
        self.tmp = self.dir / "tmp"
        self.tmp.mkdir()
        self.env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "CALLS": str(self.calls),
            "IMAGE_ROOT": str(self.image),
            "TMPDIR": str(self.tmp),
        }
        # The installation as the 0.2.0 package left it, and the files the newer manager bundles.
        self.old = {
            "compose.yml": "services:\n  manager:\n    environment:\n      AUTH_MODE: x\n",
            "deploy/Caddyfile": "https://{$DESKTOP_HOST} {\n}\n",
            ".env.example": f"PORT=8080\nMANAGER_IMAGE={REPOSITORY}:0.2.0\n",
        }
        self.write_files(self.install, self.old)
        (self.install / "compose.yml").chmod(0o640)
        (self.install / ".shipped.sha256").write_text(shipped(self.old))
        self.new = {
            "compose.yml": "services:\n  manager:\n    environment:\n      AUTH_MODE: x\n      NEW_SETTING: y\n",
            "deploy/Caddyfile": self.old["deploy/Caddyfile"],
            ".env.example": f"PORT=8080\nMANAGER_IMAGE={REPOSITORY}:latest\nNEW_SETTING=y\n",
        }
        self.write_files(self.image / "app/install", self.new)

    @staticmethod
    def write_files(directory, files):
        for name, content in files.items():
            (directory / name).parent.mkdir(parents=True, exist_ok=True)
            (directory / name).write_text(content)

    def installed(self, name):
        return (self.install / name).read_text()

    def command(self, version="", marker=None):
        """This command with the test's paths, recording its release as the package and an update do, or
        none, as the manager image bundles it. A marker makes it record each run in the call list."""
        script = self.script.replace("\nCLI_VERSION=\n", f"\nCLI_VERSION={version}\n")
        if marker:
            self.assertEqual(script.count("\nset -euo pipefail\n"), 1)
            script = script.replace("\nset -euo pipefail\n", f"\nset -euo pipefail\nprintf '%s\\n' \"{marker} $*\" >> \"$CALLS\"\n")
        return script

    def bundle(self, script):
        """Puts a command into the manager image."""
        (self.image / "app/install/bilskirnir").write_text(script)

    def update(self, env_file, release="v0.3.0", pull_status=0, **stubs):
        (self.install / ".env").write_text(env_file)
        env = {**self.env, "RELEASE": release or "", "PULL_STATUS": str(pull_status), **stubs}
        result = subprocess.run(["bash", str(self.cli), "update"], env=env, capture_output=True, text=True)
        calls = self.calls.read_text().splitlines() if self.calls.exists() else []
        return result, (self.install / ".env").read_text(), calls

    def test_a_pinned_release_moves_to_the_newest_one(self):
        result, env_file, calls = self.update(f"PORT=8080\nMANAGER_IMAGE={REPOSITORY}:0.2.0\nAUTH_MODE=none\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, f"PORT=8080\nMANAGER_IMAGE={REPOSITORY}:0.3.0\nAUTH_MODE=none\n")
        self.assertIn(f"docker pull {REPOSITORY}:0.3.0", calls)
        self.assertLess(calls.index(f"docker pull {REPOSITORY}:0.3.0"), calls.index("docker compose up -d"))
        self.assertIn("docker compose pull", calls)
        self.assertIn("0.2.0 to 0.3.0", result.stdout)

    def test_the_newest_release_stays_without_a_new_pin(self):
        before = f"MANAGER_IMAGE='{REPOSITORY}:0.3.0'\n"
        result, env_file, calls = self.update(before)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, before)
        self.assertNotIn("moves", result.stdout)
        self.assertIn("docker compose up -d", calls)

    def test_a_release_newer_than_the_newest_one_is_never_moved_back(self):
        before = f"MANAGER_IMAGE={REPOSITORY}:0.10.0\n"
        result, env_file, calls = self.update(before, release="v0.9.1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, before)
        self.assertNotIn(f"docker pull {REPOSITORY}:0.9.1", calls)
        self.assertIn(f"docker run --rm --network none --entrypoint cat {REPOSITORY}:0.10.0 /app/install/compose.yml", calls)

    def test_an_image_chosen_by_hand_is_only_pulled_again(self):
        # Even a command that records no release of its own stays, though the manager bundles a newer one.
        installed = self.command()
        self.cli.write_text(installed)
        self.bundle(self.command(marker="new command"))
        # The last one names a Docker Hub image that only looks like a release.
        for image in (f"{REPOSITORY}:latest", f"{REPOSITORY}@sha256:{'a' * 64}", "ghcr.io/someone/bilskirnir:0.1.0", "0.9.0"):
            with self.subTest(image=image):
                self.calls.unlink(missing_ok=True)
                before = f"MANAGER_IMAGE={image}\n"
                result, env_file, calls = self.update(before, release="v9.9.9")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(env_file, before)
                self.assertFalse(any(call.startswith("curl ") for call in calls))
                self.assertIn("docker compose pull", calls)
                self.assertIn("docker compose up -d", calls)
                self.assertEqual(self.cli.read_text(), installed)
                self.assertFalse(any(call.endswith("/app/install/bilskirnir") for call in calls))
                self.assertIn("command stays as it is", result.stdout)

    def test_a_missing_image_setting_is_pinned_to_the_newest_release(self):
        result, env_file, _ = self.update("AUTH_MODE=none\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, f"AUTH_MODE=none\nMANAGER_IMAGE={REPOSITORY}:0.3.0\n")

    def test_nothing_changes_when_the_release_or_its_image_is_unavailable(self):
        before = f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n"
        for release, pull_status in ((None, 0), ("nightly", 0), ("v0.3.0", 1)):
            with self.subTest(release=release, pull_status=pull_status):
                self.calls.unlink(missing_ok=True)
                result, env_file, calls = self.update(before, release=release, pull_status=pull_status)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(env_file, before)
                self.assertFalse(any(call.startswith("docker run ") for call in calls))
                self.assertEqual(self.installed("compose.yml"), self.old["compose.yml"])
                self.assertNotIn("docker compose up -d", calls)

    def test_downloaded_desktops_follow_the_release_of_the_manager(self):
        desktop = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce"
        before = f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n"
        _, _, calls = self.update(before, DOWNLOADED="1", RELEASE_LABEL="0.3.0")
        refresh = calls[calls.index("docker compose ps -q manager") + 1 :]
        self.assertEqual(
            [call for call in refresh if not call.startswith(("docker inspect", "docker image inspect"))],
            [f"docker pull {desktop}:0.3.0", f"docker tag {desktop}:0.3.0 {desktop}:24.04", f"docker image rm {desktop}:0.3.0"],
        )
        # A release tag that was already here is not the update's to remove.
        self.calls.unlink()
        _, _, calls = self.update(before, DOWNLOADED="1", RELEASE_LABEL="0.3.0", RELEASE_TAG_HERE="1")
        self.assertIn(f"docker tag {desktop}:0.3.0 {desktop}:24.04", calls)
        self.assertFalse(any(call.startswith("docker image rm ") for call in calls))
        # A manager built from master, or one too old to carry the label, downloads the reference.
        for label in ("", "<no value>"):
            with self.subTest(label=label):
                self.calls.unlink()
                _, _, calls = self.update(before, DOWNLOADED="1", RELEASE_LABEL=label)
                self.assertIn(f"docker pull {desktop}:24.04", calls)
                self.assertFalse(any(call.startswith("docker tag ") for call in calls))


    def test_the_installation_files_come_from_the_new_manager_before_it_starts(self):
        result, env_file, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
        self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])
        self.assertEqual(self.installed("deploy/Caddyfile"), self.new["deploy/Caddyfile"])
        # Like the package's, the example names the release the installation runs.
        example = f"PORT=8080\nMANAGER_IMAGE={REPOSITORY}:0.3.0\nNEW_SETTING=y\n"
        self.assertEqual(self.installed(".env.example"), example)
        self.assertEqual(self.installed(".shipped.sha256"), shipped({**self.new, ".env.example": example}))
        self.assertEqual(stat.S_IMODE((self.install / "compose.yml").stat().st_mode), 0o640)
        self.assertEqual([p.name for p in self.install.rglob("*") if p.suffix in (".bak", ".new")], [])
        self.assertEqual(list(self.tmp.iterdir()), [])
        copies = [
            i for i, call in enumerate(calls)
            if call.startswith("docker run ") and not call.endswith("/bilskirnir") and "/systemd/" not in call
        ]
        self.assertEqual(len(copies), len(FILES))
        for i in copies:
            self.assertIn(f" {REPOSITORY}:0.3.0 /app/install/", calls[i])
        self.assertLess(calls.index(f"docker pull {REPOSITORY}:0.3.0"), copies[0])
        self.assertLess(copies[-1], calls.index("docker compose pull"))
        self.assertIn("Updated compose.yml.", result.stdout)
        self.assertNotIn("Updated deploy/Caddyfile.", result.stdout)

    def test_a_file_edited_by_hand_is_saved_before_a_new_release_replaces_it(self):
        edited = self.old["compose.yml"] + '    ports: ["0.0.0.0:9000:8080"]\n'
        (self.install / "compose.yml").write_text(edited)
        result, _, _ = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])
        self.assertEqual(self.installed("compose.yml.bak"), edited)
        self.assertEqual(stat.S_IMODE((self.install / "compose.yml.bak").stat().st_mode), 0o640)
        self.assertIn("compose.yml.bak", result.stdout)
        # The next release finds the file as the update left it, replaces it and keeps the saved copy.
        newer = self.new["compose.yml"] + "      NEWER_SETTING: z\n"
        (self.image / "app/install/compose.yml").write_text(newer)
        result, _, _ = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n", release="v0.4.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installed("compose.yml"), newer)
        self.assertEqual(self.installed("compose.yml.bak"), edited)
        self.assertNotIn(".bak", result.stdout)

    def test_a_file_edited_by_hand_stays_when_the_release_brings_the_same_file(self):
        edited = self.old["deploy/Caddyfile"].replace("{\n}", "{\n    tls internal\n}")
        (self.install / "deploy/Caddyfile").write_text(edited)
        result, _, _ = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installed("deploy/Caddyfile"), edited)
        self.assertFalse((self.install / "deploy/Caddyfile.bak").exists())
        self.assertNotIn("Caddyfile", result.stdout)
        self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])

    def test_without_checksums_a_differing_file_is_saved_first(self):
        (self.install / ".shipped.sha256").unlink()
        result, _, _ = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])
        self.assertEqual(self.installed("compose.yml.bak"), self.old["compose.yml"])
        self.assertFalse((self.install / "deploy/Caddyfile.bak").exists())
        self.assertTrue((self.install / ".shipped.sha256").exists())

    def test_a_manager_without_installation_files_leaves_them_as_they_are(self):
        (self.image / "app/install/deploy/Caddyfile").unlink()
        result, env_file, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
        for name, content in self.old.items():
            self.assertEqual(self.installed(name), content)
        self.assertEqual(self.installed(".shipped.sha256"), shipped(self.old))
        self.assertIn("/app/install/deploy/Caddyfile", result.stderr)
        self.assertIn("docker compose up -d", calls)

    def bundle_units(self):
        """Puts the units that take the dashboard's update requests into the manager image."""
        for name in UNITS:
            path = self.image / "app/install/systemd" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"[Unit]\nDescription={name}\n")

    def test_a_release_brings_the_units_that_take_dashboard_requests(self):
        self.bundle_units()
        result, _, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in UNITS:
            self.assertEqual((self.units / name).read_text(), f"[Unit]\nDescription={name}\n")
            self.assertEqual(stat.S_IMODE((self.units / name).stat().st_mode), 0o644)
            self.assertIn(f"docker run --rm --network none --entrypoint cat {REPOSITORY}:0.3.0 /app/install/systemd/{name}", calls)
        # The manager offers the update once the path unit watches the folder.
        self.assertTrue((self.install / "update/watched").is_file())
        enable = calls.index("systemctl enable --now bilskirnir-update.path")
        self.assertLess(calls.index("systemctl daemon-reload"), enable)
        self.assertLess(enable, calls.index("docker compose up -d"))
        # The same units again are left as they are, so a path unit turned off by hand stays off.
        self.calls.unlink()
        result, _, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("systemctl daemon-reload", calls)
        self.assertNotIn("systemctl enable --now bilskirnir-update.path", calls)

    def test_the_units_come_only_from_releases(self):
        self.bundle_units()
        for image in (f"{REPOSITORY}:latest", "ghcr.io/someone/bilskirnir:0.1.0"):
            with self.subTest(image=image):
                self.calls.unlink(missing_ok=True)
                result, _, calls = self.update(f"MANAGER_IMAGE={image}\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(any("/app/install/systemd/" in call for call in calls))
                self.assertEqual(list(self.units.iterdir()), [])
                self.assertFalse((self.install / "update").exists())
                self.assertIn("docker compose up -d", calls)

    def test_a_release_without_the_units_leaves_dashboard_updates_as_they_are(self):
        result, _, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("updates from the dashboard stay as they are", result.stdout)
        self.assertEqual(list(self.units.iterdir()), [])
        self.assertNotIn("systemctl enable --now bilskirnir-update.path", calls)
        self.assertIn("docker compose up -d", calls)

    def test_a_newer_release_replaces_the_command_and_the_update_continues_with_it(self):
        bundled = self.command(marker="new command")
        self.bundle(bundled)
        # The new command gets the mode a command needs, whatever the old one had.
        self.cli.chmod(0o775)
        result, env_file, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n", DOWNLOADED="1", RELEASE_LABEL="0.3.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.cli.read_text(), bundled.replace("\nCLI_VERSION=\n", "\nCLI_VERSION=0.3.0\n"))
        self.assertEqual(stat.S_IMODE(self.cli.stat().st_mode), 0o755)
        self.assertIn("command moves from 0.2.0 to 0.3.0", result.stdout)
        self.assertEqual(env_file, f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
        # The new command runs the whole update again, after the manager it came from is pulled and
        # pinned: it copies the installation files by its own rules, finds itself current, recreates
        # the manager once and refreshes the desktops for it.
        restart = calls.index("new command update")
        self.assertLess(calls.index(f"docker pull {REPOSITORY}:0.3.0"), restart)
        self.assertFalse(any(call.startswith("docker compose ") for call in calls[:restart]))
        self.assertFalse(any("/app/install/compose.yml" in call for call in calls[:restart]))
        self.assertEqual(sum(call.endswith("/app/install/bilskirnir") for call in calls), 1)
        self.assertEqual(calls.count("docker compose up -d"), 1)
        desktop = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce"
        self.assertEqual(calls.count(f"docker tag {desktop}:0.3.0 {desktop}:24.04"), 1)
        self.assertLess(restart, calls.index(f"docker tag {desktop}:0.3.0 {desktop}:24.04"))
        self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])
        self.assertEqual(list(self.tmp.iterdir()), [])
        self.assertEqual(sorted(p.name for p in self.dir.glob("bilskirnir*")), ["bilskirnir"])

    def test_the_command_never_moves_to_the_same_or_an_older_release(self):
        self.bundle(self.command(marker="new command"))
        for version in ("0.3.0", "0.10.0"):
            with self.subTest(version=version):
                self.calls.unlink(missing_ok=True)
                installed = self.command(version)
                self.cli.write_text(installed)
                result, env_file, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(env_file, f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
                self.assertEqual(self.cli.read_text(), installed)
                self.assertFalse(any(call.endswith("/app/install/bilskirnir") for call in calls))
                self.assertNotIn("new command update", calls)
                self.assertIn("docker compose up -d", calls)

    def test_a_command_is_installed_only_on_a_package_with_the_level_it_needs(self):
        installed = self.cli.read_text()
        # The level the package recorded, if any, and the one the bundled command names.
        for level, needs, replaced in (
            (None, 1, True),
            (None, 2, False),
            ("2", 2, True),
            ("2", 3, False),
            ("unreadable", 2, False),
        ):
            with self.subTest(level=level, needs=needs):
                self.calls.unlink(missing_ok=True)
                self.cli.write_text(installed)
                self.level.unlink(missing_ok=True)
                if level:
                    self.level.write_text(f"{level}\n")
                bundled = re.sub(r"^NEEDS_PACKAGE=\d+$", f"NEEDS_PACKAGE={needs}", self.command(marker="new command"), flags=re.M)
                self.bundle(bundled)
                result, _, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual("new command update" in calls, replaced)
                self.assertEqual(self.cli.read_text() == installed, not replaced)
                if not replaced:
                    self.assertIn("needs a newer WSL package", result.stdout)
                # The update finishes either way, with the command the installation has afterwards.
                self.assertEqual(calls.count("docker compose up -d"), 1)
                self.assertEqual(self.installed("compose.yml"), self.new["compose.yml"])

    def test_an_incomplete_command_is_not_installed(self):
        complete = self.command(marker="new command")
        installed = self.cli.read_text()
        for name, bundled in (
            ("empty", ""),
            ("cut off", complete[: complete.index("\ncase ${1:-help} in\n")] + "\ncase x in\n"),
            ("no package level", re.sub(r"^NEEDS_PACKAGE=.*\n", "", complete, flags=re.M)),
            ("no version line", complete.replace("\nCLI_VERSION=\n", "\n")),
        ):
            with self.subTest(name):
                self.calls.unlink(missing_ok=True)
                self.bundle(bundled)
                result, env_file, calls = self.update(f"MANAGER_IMAGE={REPOSITORY}:0.2.0\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.cli.read_text(), installed)
                self.assertNotIn("new command update", calls)
                self.assertIn("bilskirnir command", result.stderr)
                self.assertEqual(env_file, f"MANAGER_IMAGE={REPOSITORY}:0.3.0\n")
                self.assertIn("docker compose up -d", calls)


# sudoedit as root: the editor works on a copy, which is written back over the original. Like sudo,
# it falls back to a default of its own, vi here, when none of the editor variables is set.
SUDO_EDIT = """#!/usr/bin/env bash
[[ $1 == --edit && $# == 2 ]] || exit 64
copy=$(mktemp)
cat "$2" > "$copy"
${SUDO_EDITOR:-${VISUAL:-${EDITOR:-vi}}} "$copy" || exit 1
chmod u+w "$2" && cat "$copy" > "$2" && chmod u-w "$2"
"""
EDITOR = """#!/usr/bin/env bash
printf 'EDITED_BY=%s\\n' "$(basename "$0")" >> "$1"
"""


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class SettingsTest(unittest.TestCase):
    """`bilskirnir settings` as the first-start user, who may read .env but not write it."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        self.cli = self.dir / "bilskirnir"
        self.cli.write_text(CLI.read_text().replace("\nINSTALL=/opt/bilskirnir\n", f"\nINSTALL={self.install}\n"))
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        (bin_dir / "sudo").write_text(SUDO_EDIT)
        for name in ("nano", "my-editor", "my-visual"):
            (bin_dir / name).write_text(EDITOR)
        for stub in bin_dir.iterdir():
            stub.chmod(0o755)
        inherited = {k: v for k, v in os.environ.items() if k not in ("EDITOR", "VISUAL", "SUDO_EDITOR")}
        self.env = {**inherited, "PATH": f"{bin_dir}:{os.environ['PATH']}"}

    def test_the_users_editor_saves_the_read_only_file(self):
        env_file = self.install / ".env"
        for editors, editor in (({}, "nano"), ({"EDITOR": "my-editor"}, "my-editor"),
                                ({"VISUAL": "my-visual", "EDITOR": "my-editor"}, "my-visual")):
            with self.subTest(editors=editors):
                if env_file.exists():
                    env_file.chmod(0o640)
                env_file.write_text("AUTH_MODE=none\n")
                env_file.chmod(0o440)
                result = subprocess.run(["bash", str(self.cli), "settings"], env={**self.env, **editors},
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(env_file.read_text(), f"AUTH_MODE=none\nEDITED_BY={editor}\n")
                self.assertIn("bilskirnir restart", result.stdout)


class PackageTest(unittest.TestCase):
    def test_the_package_pins_the_image_the_cli_updates(self):
        # wsl/Dockerfile rewrites this line to the release the package is built for, and the build
        # fails when the line is missing; an update recognises only that repository as a pin.
        lines = [line for line in (ROOT / ".env.example").read_text().splitlines() if line.startswith("MANAGER_IMAGE=")]
        self.assertEqual(len(lines), 1)
        self.assertTrue(lines[0].startswith(f"MANAGER_IMAGE={REPOSITORY}:"), lines[0])
        self.assertIn(f"\nMANAGER_REPOSITORY={REPOSITORY}\n", CLI.read_text())
        self.assertIn(f"MANAGER_IMAGE={REPOSITORY}:$BILSKIRNIR_VERSION", (ROOT / "wsl/Dockerfile").read_text())

    def test_the_manager_image_carries_the_release_label_the_cli_reads(self):
        # The manager image is built from the same argument the Rust build reads for its desktops.
        self.assertIn('LABEL bilskirnir.release="${BILSKIRNIR_RELEASE}"', (ROOT / "Dockerfile").read_text())
        self.assertIn('"bilskirnir.release"', CLI.read_text())

    def test_the_package_and_the_manager_bundle_the_files_an_update_copies(self):
        # An update copies these from /app/install in the manager image over the package's copies, and
        # compares them with the checksums the package records.
        match = re.search(r"^INSTALL_FILES=\((.*)\)$", CLI.read_text(), re.M)
        self.assertIsNotNone(match)
        self.assertEqual(tuple(match.group(1).split()), FILES)
        manager = (ROOT / "Dockerfile").read_text()
        package = (ROOT / "wsl/Dockerfile").read_text()
        for name in FILES:
            source = copy_source(manager, f"/app/install/{name}")
            self.assertIsNotNone(source, name)
            self.assertEqual(source, copy_source(package, f"/opt/bilskirnir/{name}"))
        self.assertIn(f"sha256sum {' '.join(FILES)} > .shipped.sha256", package)

    def test_the_package_and_the_manager_bundle_the_update_units(self):
        # The package installs them, and an update brings them from /app/install/systemd in the
        # manager image to installations whose package came before them.
        script = CLI.read_text()
        self.assertEqual(tuple(re.search(r"^UPDATE_UNITS=\((.*)\)$", script, re.M).group(1).split()), UNITS)
        unit_dir = re.search(r"^UNIT_DIR=(\S+)$", script, re.M).group(1)
        manager = (ROOT / "Dockerfile").read_text()
        package = (ROOT / "wsl/Dockerfile").read_text()
        for name in UNITS:
            self.assertEqual(copy_source(manager, f"/app/install/systemd/{name}"), f"wsl/{name}")
            self.assertEqual(copy_source(package, f"{unit_dir}/{name}"), f"wsl/{name}")
        self.assertRegex(package, r"systemctl enable [^\n]*\bbilskirnir-update\.path\b")

    def test_the_units_use_the_folder_and_the_names_the_manager_reads(self):
        # compose.ghcr.yml mounts the installation's update folder where src/releases.rs looks, and
        # the manager reads what the service writes there by the names that file gives them.
        script = CLI.read_text()
        install = re.search(r"^INSTALL=(\S+)$", script, re.M).group(1)
        folder = f"{install}/update"
        names = dict(re.findall(r'^pub\(crate\) const (\w+): &str = "([^"]*)";$', (ROOT / "src/releases.rs").read_text(), re.M))
        self.assertIn(f"      - ./update:{names['UPDATE_DIR']}\n", (ROOT / "compose.ghcr.yml").read_text())
        self.assertIn(f"\nPathExists={folder}/{names['REQUEST']}\n", (ROOT / "wsl/bilskirnir-update.path").read_text())
        service = (ROOT / "wsl/bilskirnir-update.service").read_text()
        self.assertIn(f"ExecStartPre=/bin/mv -f {folder}/{names['REQUEST']} {folder}/{names['STARTED']}\n", service)
        cli = re.search(r"^CLI=(\S+)$", script, re.M).group(1)
        self.assertIn(f"'exec {cli} update >{folder}/{names['LOG']} 2>&1'", service)
        self.assertIn(f"mv -f {folder}/{names['EXIT']}.new {folder}/{names['EXIT']}'", service)
        self.assertIn(f'"$INSTALL/update/{names["WATCHED"]}"', script)
        self.assertIn(f"touch {folder}/{names['WATCHED']}", (ROOT / "wsl/Dockerfile").read_text())

    def test_the_manager_bundles_the_command_the_package_installs(self):
        script = CLI.read_text()
        target = re.search(r"^CLI=(\S+)$", script, re.M).group(1)
        self.assertEqual(copy_source((ROOT / "wsl/Dockerfile").read_text(), target), "wsl/bilskirnir")
        self.assertEqual(copy_source((ROOT / "Dockerfile").read_text(), "/app/install/bilskirnir"), "wsl/bilskirnir")
        # Commands already installed read these two lines from a newer one before they install it, and
        # the package records its release in the first. Without one, any release counts as newer.
        self.assertEqual(re.findall(r"^CLI_VERSION=.*$", script, re.M), ["CLI_VERSION="])
        self.assertIn("CLI_VERSION=$BILSKIRNIR_VERSION", (ROOT / "wsl/Dockerfile").read_text())
        needs = re.findall(r"^NEEDS_PACKAGE=(\d+)$", script, re.M)
        self.assertEqual(len(needs), 1)
        # A package runs the command it ships, so it has at least the level that command needs.
        level = package_level()
        self.assertIsNotNone(level)
        self.assertEqual(f"PACKAGE_LEVEL={level[1]}", re.search(r"^PACKAGE_LEVEL=.*$", script, re.M).group(0))
        self.assertLessEqual(int(needs[0]), level[0])


COMPLETION = ROOT / "wsl/bilskirnir-completion.bash"


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class CompletionTest(unittest.TestCase):
    """Tab completion of the command's first word, with the commands the command itself lists."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        shutil.copy(CLI, self.dir / "bilskirnir")
        (self.dir / "bilskirnir").chmod(0o755)

    def complete(self, *words):
        """What Tab offers after the words typed so far, the last one being the word under the cursor."""
        script = (
            f'source "{COMPLETION}"\n'
            'COMP_WORDS=("$@"); COMP_CWORD=$(($# - 1))\n'
            '_bilskirnir "$1" "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD - 1]}"\n'
            'printf "%s\\n" "${COMPREPLY[@]}"\n'
        )
        env = {**os.environ, "PATH": f"{self.dir}:{os.environ['PATH']}"}
        result = subprocess.run(["bash", "-c", script, "bash", *words], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.split()

    def test_a_prefix_completes_to_its_command(self):
        self.assertEqual(self.complete("bilskirnir", "re"), ["restart"])
        self.assertEqual(sorted(self.complete("bilskirnir", "st")), ["start", "status", "stop"])
        self.assertEqual(self.complete("bilskirnir", "x"), [])

    def test_every_command_is_offered_and_only_the_first_word_completes(self):
        # Each branch of the command's case is offered, except help, which is what the bare command shows.
        branches = re.findall(r"^    ([a-z][a-z|-]*)\)$", CLI.read_text(), re.M)
        commands = sorted(branch for branch in branches if not branch.startswith("help"))
        self.assertIn("restart", commands)
        self.assertEqual(sorted(self.complete("bilskirnir", "")), commands)
        self.assertEqual(self.complete("bilskirnir", "restart", ""), [])

    def test_the_package_installs_the_completion_where_bash_completion_loads_it(self):
        # bash-completion loads completions/<command> the first time that command is completed.
        package = (ROOT / "wsl/Dockerfile").read_text()
        command = Path(re.search(r"^CLI=(\S+)$", CLI.read_text(), re.M).group(1)).name
        target = f"/usr/share/bash-completion/completions/{command}"
        self.assertEqual(copy_source(package, target), "wsl/bilskirnir-completion.bash")
        self.assertRegex(package, r"apt-get install [^&]*\bbash-completion\b")

    def test_the_short_name_completes_like_the_command(self):
        # b8r is a link to the command, so its help lists the same commands.
        (self.dir / "b8r").symlink_to("bilskirnir")
        self.assertEqual(self.complete("b8r", "re"), ["restart"])
        registered = subprocess.run(
            ["bash", "-c", f'source "{COMPLETION}"; complete -p b8r'], capture_output=True, text=True
        )
        self.assertEqual(registered.stdout.strip(), "complete -F _bilskirnir b8r")

    def test_the_package_links_the_short_name_and_its_completion(self):
        # Completing b8r loads completions/b8r, which is the command's completion file and registers both names.
        package = (ROOT / "wsl/Dockerfile").read_text()
        cli = Path(re.search(r"^CLI=(\S+)$", CLI.read_text(), re.M).group(1))
        completions = "/usr/share/bash-completion/completions"
        self.assertEqual(copy_source(package, f"{completions}/{cli.name}"), "wsl/bilskirnir-completion.bash")
        self.assertRegex(package, rf"\bln -s {cli.name} {cli.parent}/b8r\b")
        self.assertRegex(package, rf"\bln -s {cli.name} {completions}/b8r\b")


if __name__ == "__main__":
    unittest.main()


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class PortTest(unittest.TestCase):
    """The dashboard's address follows the PORT setting in .env, not the shell's environment."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        self.cli = self.dir / "bilskirnir"
        self.cli.write_text(CLI.read_text().replace("\nINSTALL=/opt/bilskirnir\n", f"\nINSTALL={self.install}\n"))
        # Without explorer.exe, `open` prints the address instead of opening it.
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        self.env = {"PATH": f"{bin_dir}:/usr/bin:/bin", "PORT": "7777", "HOME": str(self.dir)}

    def address(self):
        result = subprocess.run(["bash", str(self.cli), "open"], env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def test_the_address_follows_the_port_setting(self):
        (self.install / ".env").write_text("AUTH_MODE=none\nPORT='9090'\n")
        self.assertEqual(self.address(), "http://localhost:9090")

    def test_the_default_port_is_used_without_the_setting_or_the_file(self):
        self.assertEqual(self.address(), "http://localhost:8080")
        (self.install / ".env").write_text("AUTH_MODE=none\n")
        self.assertEqual(self.address(), "http://localhost:8080")


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class AccessTest(unittest.TestCase):
    """`bilskirnir status` tells who can reach the dashboard from BIND_ADDRESS, which the first start
    sets and Compose publishes the port on, with 127.0.0.1 when it is missing or empty."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        self.cli = self.dir / "bilskirnir"
        self.cli.write_text(CLI.read_text().replace("\nINSTALL=/opt/bilskirnir\n", f"\nINSTALL={self.install}\n"))
        # A running Docker and a manager that answers its health check.
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name in ("docker", "curl"):
            (bin_dir / name).write_text("#!/bin/sh\nexit 0\n")
            (bin_dir / name).chmod(0o755)
        self.env = {"PATH": f"{bin_dir}:/usr/bin:/bin", "HOME": str(self.dir), "BIND_ADDRESS": "0.0.0.0"}

    def dashboard(self, settings):
        (self.install / ".env").write_text(settings)
        result = subprocess.run(["bash", str(self.cli), "status"], env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return [line for line in result.stdout.splitlines() if line.startswith("Dashboard:")]

    def test_the_status_names_who_can_reach_the_dashboard(self):
        for settings, reach in (
            ("AUTH_MODE=none\n", "this computer only (127.0.0.1:8080)"),
            ("BIND_ADDRESS=\nPORT=9090\n", "this computer only (127.0.0.1:9090)"),
            ("BIND_ADDRESS=127.0.0.1\n", "this computer only (127.0.0.1:8080)"),
            ("BIND_ADDRESS=0.0.0.0\nPORT='9090'\n", "other devices too (0.0.0.0:9090)"),
            ("BIND_ADDRESS='192.168.1.20'\n", "other devices too (192.168.1.20:8080)"),
        ):
            with self.subTest(settings=settings):
                port = "9090" if "9090" in settings else "8080"
                self.assertEqual(self.dashboard(settings), [f"Dashboard: http://localhost:{port}, reachable from {reach}"])
