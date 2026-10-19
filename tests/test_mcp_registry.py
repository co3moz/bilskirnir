"""The desktop's MCP server declarations as the manager reads them: both folders listed, a bad
file reported instead of hiding the others, writes atomic and private, and no symlink followed
on the way in or out. Also the bilskirnir-mcp command, which writes the same files."""

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


class CommandTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.root)
        self.image = self.root / "image"
        self.target = self.root / "home/desktop/.config/bilskirnir/mcp"

    def run_command(self, *arguments, uid=1001):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            command.main(list(arguments), image=str(self.image), target=str(self.target), uid=uid)
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


if __name__ == "__main__":
    unittest.main()
