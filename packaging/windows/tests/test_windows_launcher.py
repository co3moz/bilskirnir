"""The Windows launcher, packaging/windows/thoric.ps1, which releases attach next to the package
and nothing else runs before a user does: that Windows PowerShell 5.1 reads it as written, that it
parses, and that its administrator side, the copy of itself that 'lan' starts through UAC, runs
nothing from the distribution, whose user has passwordless sudo.

The last two need PowerShell: pwsh on Linux, or Windows PowerShell from Windows or WSL.
"""

import json
from pathlib import Path
import re
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "packaging/windows/thoric.ps1"
POWERSHELL = shutil.which("pwsh") or shutil.which("powershell") or shutil.which("powershell.exe")

# Reads the script from stdin and walks from the 'elevated' command through every function of the script
# it calls, collecting their commands and strings. One line without double quotes, so it survives any
# command line.
ANALYZE = " ".join("""
$t = [Console]::In.ReadToEnd(); $e = $null;
$a = [System.Management.Automation.Language.Parser]::ParseInput($t, [ref]$null, [ref]$e);
$f = @{}; foreach ($d in $a.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true)) { $f[$d.Name] = $d };
$todo = New-Object System.Collections.Queue;
foreach ($s in $a.FindAll({ $args[0] -is [System.Management.Automation.Language.SwitchStatementAst] }, $true)) {
  foreach ($c in $s.Clauses) { if ($c.Item1.Value -eq 'elevated') { $todo.Enqueue($c.Item2) } } };
$seen = @{}; $commands = @(); $strings = @();
while ($todo.Count) { $n = $todo.Dequeue();
  foreach ($c in $n.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true)) {
    $name = $c.GetCommandName();
    if ($name) { $commands += $name; if ($f.ContainsKey($name) -and -not $seen[$name]) { $seen[$name] = 1; $todo.Enqueue($f[$name].Body) } } };
  foreach ($s in $n.FindAll({ $args[0] -is [System.Management.Automation.Language.StringConstantExpressionAst] -or
      $args[0] -is [System.Management.Automation.Language.ExpandableStringExpressionAst] }, $true)) { $strings += $s.Value } };
@{ errors = @($e | ForEach-Object { $_.Message }); commands = $commands; strings = $strings; functions = @($seen.Keys) } | ConvertTo-Json -Compress
""".split())


class WindowsLauncherTest(unittest.TestCase):
    def test_windows_powershell_reads_it_as_written(self):
        # Windows PowerShell 5.1 reads a script without a byte order mark in the ANSI code page, so any
        # character outside ASCII would reach users garbled.
        SCRIPT.read_bytes().decode("ascii")

    @unittest.skipUnless(POWERSHELL, "needs PowerShell")
    def test_the_administrator_side_runs_nothing_from_the_distribution(self):
        result = subprocess.run([POWERSHELL, "-NoProfile", "-NonInteractive", "-Command", ANALYZE],
                                input=SCRIPT.read_text(), capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["errors"], [])
        commands = [command.lower() for command in report["commands"]]
        # The walk reached the firewall and port proxy changes, so it followed the step's functions.
        self.assertIn("netsh.exe", commands)
        self.assertIn("new-netfirewallhypervrule", commands)
        self.assertEqual([command for command in commands if re.fullmatch(r"wsl(\.exe)?", command)], [])
        # The LAN task's command is a string, so no string may start wsl either.
        self.assertEqual([text for text in report["strings"] if re.search(r"(?i)\bwsl(\.exe)?(\s|$)", text)], [])


if __name__ == "__main__":
    unittest.main()
