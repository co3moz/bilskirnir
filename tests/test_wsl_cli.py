"""The WSL package's `bilskirnir update`: which manager release it moves an installation to, and
which desktop images it refreshes for that manager.

The script runs against a temporary installation directory with stand-ins for docker, curl,
sudo and systemctl on PATH, each recording its arguments.
"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "wsl/bilskirnir"
REPOSITORY = "ghcr.io/co3moz/bilskirnir"

STUB = """#!/usr/bin/env bash
printf '%s\\n' "$(basename "$0") $*" >> "$CALLS"
case "$(basename "$0") $*" in
    "curl "*) [[ -n ${RELEASE:-} ]] || exit 22; printf '{\\n  "url": "x",\\n  "tag_name": "%s",\\n  "name": "%s"\\n}\\n' "$RELEASE" "$RELEASE" ;;
    "docker pull "*) exit "${PULL_STATUS:-0}" ;;
    "docker compose exec "*) printf '[{"reference": "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"}]\\n' ;;
    "docker compose ps "*) echo manager-container ;;
    "docker inspect "*) printf '%s\\n' "${RELEASE_LABEL-<no value>}" ;;
    "docker image inspect "*":24.04") [[ -n ${DOWNLOADED:-} ]] || exit 1 ;;
    "docker image inspect "*) [[ -n ${RELEASE_TAG_HERE:-} ]] || exit 1 ;;
    "sudo "*) exec "$@" ;;
esac
exit 0
"""


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class UpdateTest(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        script = CLI.read_text()
        self.assertEqual(script.count("\nINSTALL=/opt/bilskirnir\n"), 1)
        self.cli = self.dir / "bilskirnir"
        self.cli.write_text(script.replace("\nINSTALL=/opt/bilskirnir\n", f"\nINSTALL={self.install}\n"))
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name in ("docker", "curl", "sudo", "systemctl"):
            (bin_dir / name).write_text(STUB)
            (bin_dir / name).chmod(0o755)
        self.calls = self.dir / "calls"
        self.env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "CALLS": str(self.calls)}

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
        self.assertNotIn(f"docker pull {REPOSITORY}:0.3.0", calls)
        self.assertIn("docker compose up -d", calls)

    def test_a_release_newer_than_the_newest_one_is_never_moved_back(self):
        before = f"MANAGER_IMAGE={REPOSITORY}:0.10.0\n"
        result, env_file, calls = self.update(before, release="v0.9.1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(env_file, before)
        self.assertFalse(any(call.startswith("docker pull ghcr.io/co3moz/bilskirnir:") for call in calls))

    def test_an_image_chosen_by_hand_is_only_pulled_again(self):
        for image in (f"{REPOSITORY}:latest", f"{REPOSITORY}@sha256:{'a' * 64}", "ghcr.io/someone/bilskirnir:0.1.0"):
            with self.subTest(image=image):
                self.calls.unlink(missing_ok=True)
                before = f"MANAGER_IMAGE={image}\n"
                result, env_file, calls = self.update(before, release="v9.9.9")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(env_file, before)
                self.assertFalse(any(call.startswith("curl ") for call in calls))
                self.assertIn("docker compose pull", calls)
                self.assertIn("docker compose up -d", calls)

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
        self.assertIn('LABEL io.wsl-desktop.release="${BILSKIRNIR_RELEASE}"', (ROOT / "Dockerfile").read_text())
        self.assertIn('"io.wsl-desktop.release"', CLI.read_text())


if __name__ == "__main__":
    unittest.main()
