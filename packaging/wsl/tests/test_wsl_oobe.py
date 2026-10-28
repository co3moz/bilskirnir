"""The WSL package's first start with piped answers, as an automated installation runs it: which
settings the answers write to .env, above all who can reach the dashboard, on which port and by which
names, and what the setup says about it, including the Windows step for the WSL networking mode in use.

The script runs against a temporary installation directory and wsl.conf, with stand-ins on PATH for
the commands that change the system (useradd, chpasswd, chown), for the thoric command and for
wslinfo, which answers the networking mode from the NETWORKING variable.
"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
OOBE = ROOT / "packaging/wsl/oobe.sh"

# chpasswd reads its input to the end, as the real one does. A stand-in that exits first leaves the
# script's `printf ... | chpasswd` writing into a closed pipe, and pipefail ends the setup with 141.
STUB = """#!/usr/bin/env bash
printf '%s\\n' "$(basename "$0") $*" >> "$CALLS"
case "$(basename "$0")" in
    chpasswd) cat >> "$CALLS.chpasswd" ;;
    getent) exit 2 ;;
    wslinfo) [[ -n ${NETWORKING:-} ]] || exit 1; echo "$NETWORKING" ;;
esac
exit 0
"""

LAN_ENABLE = "\n  irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1 | iex\n"


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "needs bash on a POSIX system")
class FirstStartTest(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.install = self.dir / "install"
        self.install.mkdir()
        shutil.copy(ROOT / ".env.example", self.install / ".env.example")
        self.wsl_conf = self.dir / "wsl.conf"
        self.wsl_conf.write_text("[boot]\nsystemd=true\n")
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name in ("getent", "useradd", "chpasswd", "chown", "runuser", "thoric", "wslinfo"):
            (bin_dir / name).write_text(STUB)
            (bin_dir / name).chmod(0o755)
        script = OOBE.read_text()
        self.assertEqual(script.count("\nINSTALL=/opt/thoric\n"), 1)
        script = script.replace("\nINSTALL=/opt/thoric\n", f"\nINSTALL={self.install}\n")
        for path, replacement in (("/etc/wsl.conf", self.wsl_conf), ("/usr/local/bin/thoric", bin_dir / "thoric")):
            self.assertIn(path, script)
            script = script.replace(path, str(replacement))
        self.script = self.dir / "oobe.sh"
        self.script.write_text(script)
        self.calls = self.dir / "calls"
        self.env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "CALLS": str(self.calls)}
        self.env.pop("NETWORKING", None)

    def first_start(self, answers, networking=None):
        env = {**self.env, **({"NETWORKING": networking} if networking else {})}
        return subprocess.run(["bash", str(self.script)], input=answers, env=env, capture_output=True, text=True)

    def settings(self):
        lines = (self.install / ".env").read_text().splitlines()
        return dict(line.split("=", 1) for line in lines if "=" in line and not line.startswith("#"))

    def test_only_this_computer_reaches_the_dashboard_by_default(self):
        # Answers end after the password, so every menu takes its default.
        result = self.first_start("alice\nsecret123\nsecret123\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = self.settings()
        self.assertEqual(settings["BIND_ADDRESS"], "127.0.0.1")
        # Root agent access is on by default, under the name it has had since 0.6.3.
        self.assertEqual(settings["ROOT_MCP_ENABLED"], "true")
        self.assertNotIn("MCP_ENABLED", settings)
        self.assertIn("Who can reach the dashboard? This computer only", result.stdout)
        self.assertEqual(settings["PORT"], "8080")
        self.assertIn("listens on 127.0.0.1:8080", result.stdout)
        # Names are asked for only when other devices reach the dashboard.
        self.assertEqual(settings["ALLOWED_HOSTS"], "localhost")
        self.assertNotIn("open it by a name", result.stdout)
        self.assertNotIn("Windows has to let other devices in", result.stdout)
        self.assertNotIn("thoric.ps1", result.stdout)
        # The password reaches chpasswd on its input, never on a command line.
        self.assertEqual((self.dir / "calls.chpasswd").read_text(), "alice:secret123\n")
        self.assertNotIn("secret123", self.calls.read_text())

    def test_other_devices_get_the_windows_step_of_the_networking_mode(self):
        env_example = self.install / ".env.example"
        env_example.write_text(env_example.read_text().replace("\nPORT=8080\n", "\nPORT=9090\n"))
        for networking, expected, unexpected in (
            ("mirrored", "so port 9090 needs a Hyper-V firewall rule.", "NAT networking"),
            ("nat", "so port 9090 needs a port proxy to this distribution and a Windows firewall rule.", "Hyper-V"),
            (None, "What that takes depends on the WSL networking mode.", "Hyper-V"),
        ):
            with self.subTest(networking=networking):
                (self.install / ".env").unlink(missing_ok=True)
                result = self.first_start("alice\nsecret123\nsecret123\nyes\nrandom\nother\n", networking)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.settings()["BIND_ADDRESS"], "0.0.0.0")
                self.assertIn("Who can reach the dashboard? Other devices too", result.stdout)
                self.assertIn("listens on 0.0.0.0:9090", result.stdout)
                self.assertIn(expected, result.stdout)
                self.assertNotIn(unexpected, result.stdout)
                # The Windows helper sets up every mode, and the guide has the steps by hand.
                self.assertIn(LAN_ENABLE, result.stdout)
                self.assertIn("WSL_PACKAGE.md#access-from-other-devices", result.stdout)

    def test_the_port_and_the_names_go_into_the_settings(self):
        result = self.first_start("alice\nsecret123\nsecret123\nyes\nrandom\nother\n9191\nMy-PC, my-pc.local\n", "nat")
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = self.settings()
        self.assertEqual((settings["PORT"], settings["ALLOWED_HOSTS"]), ("9191", "localhost,my-pc,my-pc.local"))
        self.assertIn("listens on 0.0.0.0:9191", result.stdout)
        self.assertIn("it answers to my-pc, my-pc.local.", result.stdout)
        self.assertIn("so port 9191 needs a port proxy", result.stdout)
        # This computer alone gets the port without the names question, which would take the next line.
        (self.install / ".env").unlink()
        result = self.first_start("alice\nsecret123\nsecret123\nyes\nrandom\nthis\n9191\nmy-pc\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = self.settings()
        self.assertEqual((settings["PORT"], settings["ALLOWED_HOSTS"]), ("9191", "localhost"))
        self.assertIn("listens on 127.0.0.1:9191", result.stdout)

    def test_an_unusable_port_or_name_ends_the_setup_before_anything_is_created(self):
        for answers, message in (
            ("this\n0\n", "Use a port number from 1 to 65535."),
            ("this\n65536\n", "Use a port number from 1 to 65535."),
            ("this\n8080x\n", "Use a port number from 1 to 65535."),
            ("other\n\nmy_pc\n", "Use letters, digits, dots and dashes"),
            ("other\n\nmy-pc,,nas\n", "Use letters, digits, dots and dashes"),
            ("other\n\nmy pc\n", "Use letters, digits, dots and dashes"),
            ("other\n\n-my-pc\n", "Use letters, digits, dots and dashes"),
        ):
            with self.subTest(answers=answers):
                result = self.first_start("alice\nsecret123\nsecret123\nyes\nrandom\n" + answers)
                self.assertEqual(result.returncode, 1)
                self.assertIn(message, result.stdout)
                self.assertFalse((self.install / ".env").exists())
                self.assertNotIn("useradd", self.calls.read_text() if self.calls.exists() else "")

    def test_the_question_warns_when_nothing_guards_the_address(self):
        # No password, agent access off: the first letter answers the network question.
        result = self.first_start("alice\n\nno\no\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = self.settings()
        self.assertEqual((settings["AUTH_MODE"], settings["ROOT_MCP_ENABLED"], settings["BIND_ADDRESS"]),
                         ("none", "false", "0.0.0.0"))
        # Without a password the account gets none.
        self.assertNotIn("chpasswd", self.calls.read_text())
        before, after = result.stdout.split("Who can reach the dashboard?", 1)
        self.assertIn("There is no password, so any device that reaches the dashboard controls every desktop.", before)
        self.assertIn("on the network who reaches it controls every desktop", after)
        # Agents without a token are warned about too, also with a password.
        result = self.first_start("alice\nsecret123\nsecret123\nyes\nnone\nthis\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.settings()["BIND_ADDRESS"], "127.0.0.1")
        before = result.stdout.split("Who can reach the dashboard?", 1)[0]
        self.assertIn("Agents need no token, so any device that reaches the agent endpoint controls every desktop.", before)
        self.assertNotIn("There is no password", before)

    def test_an_unknown_answer_ends_the_setup_before_anything_is_created(self):
        result = self.first_start("alice\nsecret123\nsecret123\nyes\nrandom\neveryone\n")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Who can reach the dashboard? Unknown answer: everyone", result.stdout)
        self.assertFalse((self.install / ".env").exists())
        self.assertNotIn("useradd", self.calls.read_text() if self.calls.exists() else "")


if __name__ == "__main__":
    unittest.main()
