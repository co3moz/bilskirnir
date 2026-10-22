"""The desktop's MCP server declarations as the manager reads them: both folders listed, a bad
file reported instead of hiding the others, writes atomic and private, and no symlink followed
on the way in or out; the status the manager sends kept for the desktop while it watches. Also
the bilskirnir-mcp command, which writes the same files and reads that status."""

import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


registry = load("mcp_registry", ROOT / "src/mcp_registry.py")
command = load("bilskirnir_mcp", ROOT / "desktop/bilskirnir-mcp.py")


class Output:
    def __init__(self):
        self.buffer = io.BytesIO()
        self.text = io.StringIO()

    def write(self, text):
        self.text.write(text)

    def flush(self):
        pass


class RegistryTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)
        self.image = self.root / "etc/bilskirnir/mcp.d"
        self.container = self.root / "home/desktop/.config/bilskirnir/mcp"
        self.image.mkdir(parents=True)
        for name, value in (("IMAGE", self.image), ("CONTAINER", self.container)):
            patch = mock.patch.object(registry, name, str(value))
            patch.start()
            self.addCleanup(patch.stop)

    def run_helper(self, *arguments, stdin=b""):
        output = Output()
        source = mock.Mock()
        source.buffer = io.BytesIO(stdin)
        with mock.patch.object(registry.sys, "stdout", output), mock.patch.object(registry.sys, "stdin", source):
            registry.main(list(arguments))
        return [json.loads(line) for line in output.text.getvalue().splitlines()]

    def test_both_folders_are_listed_and_a_bad_file_says_why(self):
        (self.image / "chrome.json").write_text(json.dumps({"command": "playwright-mcp"}))
        (self.image / "broken.json").write_text("{not json")
        (self.image / "Bad Name.json").write_text("{}")
        (self.image / "notes.txt").write_text("ignored")
        self.container.mkdir(parents=True)
        (self.container / "x.json").write_text(json.dumps({"url": "http://127.0.0.1:9000/mcp"}))
        servers = self.run_helper("list")[0]["servers"]
        by_name = {(s["name"], s["source"]): s for s in servers}
        self.assertEqual(by_name[("chrome", "image")]["spec"], {"command": "playwright-mcp"})
        self.assertIn("not valid JSON", by_name[("broken", "image")]["error"])
        self.assertIn("error", by_name[("Bad Name", "image")])
        self.assertEqual(by_name[("x", "container")]["spec"]["url"], "http://127.0.0.1:9000/mcp")
        self.assertNotIn(("notes", "image"), by_name)

    def test_a_missing_folder_lists_nothing(self):
        shutil.rmtree(self.image)
        self.assertEqual(self.run_helper("list")[0], {"servers": []})

    def test_a_symlinked_declaration_or_folder_is_not_followed(self):
        secret = self.root / "secret.json"
        secret.write_text(json.dumps({"command": "leak"}))
        (self.image / "link.json").symlink_to(secret)
        servers = self.run_helper("list")[0]["servers"]
        self.assertEqual(len(servers), 1)
        self.assertIn("error", servers[0])
        self.assertNotIn("spec", servers[0])
        # A symlinked desktop folder is not written through either.
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        self.container.parent.mkdir(parents=True)
        self.container.symlink_to(elsewhere)
        with self.assertRaises(OSError):
            self.run_helper("write", "x", stdin=b'{"command": "x"}')
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_a_write_creates_private_folders_and_files_atomically(self):
        self.assertEqual(self.run_helper("write", "x", stdin=b'{"command": "node"}'), [{"written": "x"}])
        written = self.container / "x.json"
        self.assertEqual(json.loads(written.read_text()), {"command": "node"})
        self.assertEqual(written.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.container.stat().st_mode & 0o777, 0o700)
        self.assertEqual([p.name for p in self.container.iterdir()], ["x.json"])
        self.run_helper("write", "x", stdin=b'{"command": "deno"}')
        self.assertEqual(json.loads(written.read_text()), {"command": "deno"})
        for name, data in (("Bad", b"{}"), ("x", b"not json"), ("x", b"{" * 70000)):
            with self.assertRaises(ValueError):
                self.run_helper("write", name, stdin=data)
        self.assertEqual(json.loads(written.read_text()), {"command": "deno"})

    def test_remove_deletes_only_the_desktop_users_file(self):
        self.run_helper("write", "x", stdin=b'{"command": "node"}')
        self.assertEqual(self.run_helper("remove", "x"), [{"removed": "x"}])
        self.assertFalse((self.container / "x.json").exists())
        with self.assertRaises(FileNotFoundError):
            self.run_helper("remove", "x")
        (self.image / "chrome.json").write_text("{}")
        with self.assertRaises(FileNotFoundError):
            self.run_helper("remove", "chrome")
        self.assertTrue((self.image / "chrome.json").exists())

    def test_the_signature_changes_with_every_file(self):
        before = registry.signature(str(self.image))
        (self.image / "chrome.json").write_text("{}")
        after = registry.signature(str(self.image))
        self.assertNotEqual(before, after)
        self.assertIsNone(registry.signature(str(self.root / "missing")))


class WatchTest(unittest.TestCase):
    """The watcher keeps the status the manager sends in the desktop's runtime folder while the
    manager is there, and nothing else reaches stdout."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)
        self.runtime = self.root / "run/desktop"
        self.runtime.mkdir(parents=True)
        self.status = self.runtime / "bilskirnir/mcp-status.json"
        for name, value in (("IMAGE", self.root / "image"), ("CONTAINER", self.root / "mine"),
                            ("STATUS", self.runtime / "bilskirnir")):
            patch = mock.patch.object(registry, name, str(value))
            patch.start()
            self.addCleanup(patch.stop)

    def start(self):
        reading, self.writing = os.pipe()
        self.addCleanup(os.close, reading)
        source = mock.Mock()
        source.fileno.return_value = reading
        self.output = Output()
        patches = [mock.patch.object(registry.sys, "stdout", self.output),
                   mock.patch.object(registry.sys, "stdin", source)]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.thread = threading.Thread(target=registry.watch, daemon=True)
        self.thread.start()
        # Runs first, so a failed test still ends the watcher before its pipe closes.
        self.addCleanup(self.stop)

    def send(self, data):
        os.write(self.writing, data)

    def stop(self):
        if self.writing is not None:
            os.close(self.writing)
            self.writing = None
        self.thread.join(10)
        self.assertFalse(self.thread.is_alive())

    def wait_for(self, check):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if check():
                return
            time.sleep(0.02)
        self.fail("timed out")

    def read_status(self):
        try:
            return json.loads(self.status.read_text())
        except (OSError, ValueError):
            return None

    def test_the_latest_status_is_kept_privately_until_stdin_closes(self):
        self.start()
        self.send(b'{"status": {"servers": [{"name": "a"}]}}\n{"status": {"servers": [{"name": ')
        self.wait_for(lambda: self.read_status() == {"servers": [{"name": "a"}]})
        self.send(b'"b"}]}}\nnot json\n{"other": 1}\n')
        self.wait_for(lambda: self.read_status() == {"servers": [{"name": "b"}]})
        self.assertEqual(self.status.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.status.parent.stat().st_mode & 0o777, 0o700)
        self.stop()
        self.assertFalse(self.status.exists())
        self.assertEqual([p.name for p in self.status.parent.iterdir()], [])
        # stdout carried the listing alone.
        self.assertEqual([json.loads(line) for line in self.output.text.getvalue().splitlines()],
                         [{"servers": []}])

    def test_a_line_over_the_limit_is_dropped_and_the_next_one_kept(self):
        with mock.patch.object(registry, "STATUS_LIMIT", 64):
            self.start()
            self.send(b'{"status": {"x": "' + b"y" * 200)
            self.send(b'"}}\n{"status": {"kept": true}}\n')
            self.wait_for(lambda: self.read_status() == {"kept": True})
            self.stop()

    def test_a_status_written_by_another_watcher_is_left_alone(self):
        self.start()
        self.send(b'{"status": {"mine": true}}\n')
        self.wait_for(lambda: self.read_status() == {"mine": True})
        replacement = self.status.with_name("replacement")
        replacement.write_text('{"newer": true}')
        os.rename(replacement, self.status)
        self.stop()
        self.assertEqual(self.read_status(), {"newer": True})

    def test_a_symlinked_status_folder_is_not_written_through(self):
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        (self.runtime / "bilskirnir").symlink_to(elsewhere)
        self.start()
        self.send(b'{"status": {"leak": true}}\n')
        time.sleep(0.3)
        self.stop()
        self.assertEqual(list(elsewhere.iterdir()), [])


class CommandTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)
        self.image = self.root / "image"
        self.target = self.root / "home/desktop/.config/bilskirnir/mcp"

    def run_command(self, *arguments, uid=1001):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            command.main(list(arguments), image=str(self.image), target=str(self.target), uid=uid,
                         status_path=str(self.root / "run/bilskirnir/mcp-status.json"))
        return output.getvalue()

    def read(self, name):
        return json.loads((self.target / (name + ".json")).read_text())

    def test_a_program_is_registered_with_its_arguments_and_variables(self):
        self.run_command("add", "x", "--description", "My extension", "--env", "PORT=9", "--cwd",
                         "/home/desktop/ext", "--", "node", "mcp.js", "--verbose")
        self.assertEqual(self.read("x"), {"description": "My extension", "command": "node",
                                          "args": ["mcp.js", "--verbose"], "env": {"PORT": "9"},
                                          "cwd": "/home/desktop/ext"})
        self.assertEqual((self.target / "x.json").stat().st_mode & 0o777, 0o600)

    def test_an_http_server_is_registered_with_its_headers(self):
        self.run_command("add", "api", "--url", "http://127.0.0.1:8931/mcp", "--header",
                         "Authorization: Bearer ${TOKEN}")
        self.assertEqual(self.read("api"), {"url": "http://127.0.0.1:8931/mcp",
                                            "headers": {"Authorization": "Bearer ${TOKEN}"}})

    def test_mistakes_are_refused_before_anything_is_written(self):
        for arguments in (("add", "Bad", "--", "node"), ("add", "x"),
                          ("add", "x", "--url", "http://127.0.0.1:1/", "--", "node"),
                          ("add", "x", "--header", "A: b", "--", "node"),
                          ("add", "x", "--env", "NOVALUE", "--", "node")):
            with self.assertRaises(ValueError, msg=arguments):
                self.run_command(*arguments)
        with self.assertRaises(ValueError):
            self.run_command("add", "x", "--", "node", uid=0)
        self.assertFalse(self.target.exists())

    def test_list_shows_both_folders_and_remove_takes_the_desktop_users(self):
        self.image.mkdir()
        (self.image / "chrome.json").write_text(json.dumps({"command": "playwright-mcp", "args": ["--x"]}))
        self.run_command("add", "x", "--", "node", "mcp.js")
        shown = self.run_command("list")
        self.assertIn("chrome", shown)
        self.assertIn("playwright-mcp --x", shown)
        self.assertIn("node mcp.js", shown)
        self.run_command("remove", "x")
        self.assertNotIn("node mcp.js", self.run_command("list"))

    def test_an_address_or_name_the_manager_would_refuse_is_refused_here(self):
        for arguments in (("add", "x", "--url", "http://example.com:80/mcp"),
                          ("add", "x", "--url", "http://127.0.0.1/mcp"),
                          ("add", "x", "--url", "http://127.0.0.1:0/mcp"),
                          ("add", "x", "--url", "http://127.0.0.1:99999/mcp"),
                          ("add", "x", "--url", "http://127.0.0.1:1/a b"),
                          ("add", "x", "--url", "http://127.0.0.1:1/mcp", "--header", "Bad Name: 1"),
                          ("add", "x", "--env", "1ST=x", "--", "node")):
            with self.assertRaises(ValueError, msg=arguments):
                self.run_command(*arguments)
        self.assertFalse(self.target.exists())
        for url in ("http://localhost:8931/mcp", "https://[::1]:1", "http://127.0.0.1:${PORT}/mcp"):
            self.run_command("add", "x", "--url", url)
            self.assertEqual(self.read("x")["url"], url)

    def test_list_json_says_which_declaration_is_used_and_what_bilskirnir_knows(self):
        self.image.mkdir()
        (self.image / "chrome.json").write_text(json.dumps({"command": "playwright-mcp"}))
        (self.image / "unity.json").write_text(json.dumps({"url": "http://127.0.0.1:8080/mcp"}))
        self.run_command("add", "chrome", "--", "playwright-mcp", "--headless")
        unknown = json.loads(self.run_command("list", "--json"))
        self.assertEqual(unknown["watched"], False)
        self.assertNotIn("endpoints", unknown)
        self.assertEqual([(s["name"], s["source"], s["active"]) for s in unknown["servers"]],
                         [("chrome", "image", False), ("unity", "image", True), ("chrome", "container", True)])
        self.assertEqual(unknown["servers"][2]["spec"]["args"], ["--headless"])
        self.assertTrue(all("status" not in s for s in unknown["servers"]))

        status = self.root / "run/bilskirnir/mcp-status.json"
        status.parent.mkdir(parents=True)
        endpoint = {"name": "desk", "path": "/mcp/desk", "has_token": False, "scopes": ["view"]}
        status.write_text(json.dumps({"status_at": 7, "endpoints": [endpoint], "servers": [
            {"name": "chrome", "source": "container", "state": "ready", "tools": [{"name": "click"}],
             "tools_at": 6, "error": None, "log": ""},
            {"name": "unity", "source": "image", "state": "error", "error": "refused", "tools": []}]}))
        known = json.loads(self.run_command("list", "--json"))
        self.assertEqual((known["watched"], known["status_at"], known["endpoints"]), (True, 7, [endpoint]))
        by_key = {(s["name"], s["source"]): s for s in known["servers"]}
        self.assertNotIn("status", by_key[("chrome", "image")])
        self.assertEqual(by_key[("chrome", "container")]["status"]["tools"], [{"name": "click"}])
        self.assertEqual(by_key[("unity", "image")]["status"]["error"], "refused")
        # A status file that is not JSON is as good as none.
        status.write_text("{broken")
        self.assertEqual(json.loads(self.run_command("list", "--json"))["watched"], False)

    def test_the_command_reads_the_status_where_the_watcher_writes_it(self):
        self.assertEqual(os.path.join(registry.STATUS, registry.STATUS_FILE), command.STATUS)
        self.assertEqual(registry.STATUS_LIMIT, command.STATUS_LIMIT)

    def test_show_prints_the_used_declaration_as_a_template_holds_it(self):
        self.image.mkdir()
        (self.image / "chrome.json").write_text(json.dumps({"command": "playwright-mcp"}))
        self.assertEqual(json.loads(self.run_command("show", "chrome")),
                         {"name": "chrome", "command": "playwright-mcp"})
        self.run_command("add", "chrome", "--description", "Mine", "--", "node", "x.js")
        self.assertEqual(json.loads(self.run_command("show", "chrome")),
                         {"name": "chrome", "description": "Mine", "command": "node", "args": ["x.js"]})
        # root may read, as with list.
        self.assertIn("node", self.run_command("show", "chrome", uid=0))
        with self.assertRaises(FileNotFoundError):
            self.run_command("show", "missing")


if __name__ == "__main__":
    unittest.main()
