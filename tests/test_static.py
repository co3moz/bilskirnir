"""What a Python file can get wrong before it runs, read without running it. The unit tests import
only the helpers they test, and tests/integration.py runs once at the end of a piece of work, so
nothing else reads every file: each one compiles, with the compiler's warnings, such as an invalid
escape or an assert on a tuple, as errors. The acceptance suite is one flat script whose phases share
its helpers, so it never binds a helper's name again: a phase that assigned `refused` once hid the
suite's refused() from every phase after it.
"""

import ast
import os
from pathlib import Path
import subprocess
import textwrap
import unittest
import warnings

ROOT = Path(__file__).resolve().parents[1]
# The folders a walk of the checkout leaves out: git's own, what .gitignore names, installed packages
# and the worktrees of other branches.
GENERATED = {".git", ".claude", "node_modules", "__pycache__", "dist", "target", "data"}
# The scripts run by hand, whose helpers serve every part of the script after them.
SCRIPTS = ("tests/integration.py", "tests/cleanup_acceptance.py")


def repository_files():
    """Every file git tracks in the checkout. One git cannot read, such as a Windows worktree seen
    from WSL, is walked instead, without the generated folders."""
    try:
        listed = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        files = []
        for folder, subfolders, names in os.walk(ROOT):
            subfolders[:] = [name for name in subfolders if name not in GENERATED]
            files.extend(Path(folder, name) for name in names)
        return files
    return [ROOT / name for name in listed.stdout.decode().split("\0") if name]


def is_python(path):
    if path.suffix == ".py":
        return True
    with path.open("rb") as file:
        first = file.readline(200)
    return first.startswith(b"#!") and b"python" in first


def python_files():
    return sorted(path for path in repository_files() if path.is_file() and is_python(path))


def module_scope(body):
    """The statements of a module's own scope, also those in its top-level try, with, if, loop and
    match blocks, which bind names of the module as well."""
    for node in body:
        yield node
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            for field in ("body", "orelse", "finalbody"):
                yield from module_scope(getattr(node, field, []))
            for block in (*getattr(node, "handlers", []), *getattr(node, "cases", [])):
                yield from module_scope(block.body)


def bindings(tree):
    """Every name the code binds, in any scope, with the node that binds it."""
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            yield node.name, node
        elif isinstance(node, ast.alias):
            yield node.asname or node.name.split(".")[0], node
        elif isinstance(node, ast.Name) and not isinstance(node.ctx, ast.Load):
            yield node.id, node
        elif isinstance(node, ast.arg):
            yield node.arg, node
        elif isinstance(node, (ast.ExceptHandler, ast.MatchAs, ast.MatchStar)) and node.name:
            yield node.name, node
        elif isinstance(node, ast.MatchMapping) and node.rest:
            yield node.rest, node


def imported(tree):
    """The module or the name each import binds, by the alias that binds it."""
    targets = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            targets.update((alias, alias.name if alias.asname else alias.name.split(".")[0]) for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            module = "." * node.level + (node.module or "")
            targets.update((alias, f"{module}:{alias.name}") for alias in node.names)
    return targets


def rebound_helpers(source):
    """Each place a script binds again a name its own scope gave a function, a class or a module, as
    (line, name, line of the helper). Importing the module the name already holds, as
    `import urllib.parse` after `import urllib.request` does, binds nothing new."""
    tree = ast.parse(source)
    helpers = {}
    for node in module_scope(tree.body):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            helpers.setdefault(node.name, node)
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            for alias in node.names:
                helpers.setdefault(alias.asname or alias.name.split(".")[0], alias)
    targets = imported(tree)
    return sorted(
        (node.lineno, name, helpers[name].lineno)
        for name, node in bindings(tree)
        if name in helpers
        and node is not helpers[name]
        and (node not in targets or targets[node] != targets.get(helpers[name]))
    )


class CompileTest(unittest.TestCase):
    def test_every_python_file_compiles_without_warnings(self):
        files = python_files()
        # The listing must reach the files no other test imports, or this test checks nothing.
        unread = {ROOT / script for script in SCRIPTS} | set((ROOT / "core/manager/helpers").glob("*.py"))
        self.assertLessEqual(unread, set(files))
        for path in files:
            with self.subTest(path=path.relative_to(ROOT).as_posix()), warnings.catch_warnings():
                warnings.simplefilter("error")
                compile(path.read_bytes(), str(path), "exec", dont_inherit=True)


class ScriptHelpersTest(unittest.TestCase):
    def test_the_scripts_never_bind_a_helper_name_again(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                found = rebound_helpers((ROOT / script).read_text(encoding="utf-8"))
                self.assertEqual(
                    [f"{script}:{line} binds {name} again, the helper of line {helper}" for line, name, helper in found],
                    [],
                )

    def test_every_way_to_bind_a_name_again_is_found(self):
        # Each line marked `again:` binds the names after it again; reading a helper binds nothing.
        source = textwrap.dedent("""\
            import json
            import urllib.request
            import urllib.parse
            from os import path as where
            def refused(response):
                return response
            class Socket:
                pass
            try:
                refused = refused(1)  # again: refused
                for json in []:  # again: json
                    pass
                def phase(Socket, *where):  # again: Socket where
                    import json
                    from os import path as where
                    refused = 2  # again: refused
                    return [json for json in where]  # again: json
                with open("x") as where:  # again: where
                    pass
                values = [lambda refused: 1, (Socket := 2)]  # again: refused Socket
                match refused:
                    case {"key": json, **where}:  # again: json where
                        pass
                    case [*Socket]:  # again: Socket
                        pass
                def refused():  # again: refused
                    pass
                import json as Socket  # again: Socket
                import os.path as json  # again: json
                class where:  # again: where
                    pass
            except Exception as json:  # again: json
                del refused  # again: refused
            print(json.dumps(where), refused(json), Socket().refused, dict(refused=1), urllib.parse)
        """)
        lines = source.splitlines()
        marked = sorted(
            (number, name)
            for number, line in enumerate(lines, 1)
            if "# again:" in line
            for name in line.split("# again:")[1].split()
        )
        found = rebound_helpers(source)
        self.assertEqual([(line, name) for line, name, _ in found], marked)
        # Each names the helper's first definition.
        self.assertEqual({helper for _, name, helper in found if name == "refused"}, {lines.index("def refused(response):") + 1})
