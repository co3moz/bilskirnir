<#
.SYNOPSIS
Install, start, stop and auto-start the Bilskirnir WSL distribution from Windows.

.DESCRIPTION
Usage: .\bilskirnir.ps1 <command> [options]

  install -Package .\bilskirnir.wsl   Register the distribution and run its first-start setup
  start                               Boot the distribution, keep it running and start the manager
  stop                                Stop the manager and shut the distribution down
  status                              Show the manager state
  open                                Open the dashboard in the browser
  autostart enable|disable            Start Bilskirnir at Windows sign-in through a scheduled task
  uninstall                           Remove the auto-start task and unregister the distribution

WSL stops a distribution shortly after its last Windows-started process ends, so 'start' and the
auto-start task keep a hidden keep-alive process open while Docker and the manager run inside.
#>
param(
    [Parameter(Position = 0)][string]$Command = "help",
    [Parameter(Position = 1)][string]$Option = "",
    [string]$Package = "",
    [string]$Distro = "Bilskirnir",
    [int]$Port = 8080
)
$ErrorActionPreference = "Stop"
$taskName = "Bilskirnir keep-alive"

function Invoke-Wsl { param([string[]]$Arguments) & wsl.exe @Arguments; if ($LASTEXITCODE -ne 0) { throw "wsl.exe exited with code $LASTEXITCODE" } }
function Test-Registered { $list = (& wsl.exe --list --quiet) -join "`n"; return ($list -replace "`0", "") -match "(?m)^$Distro\s*$" }
function Start-KeepAlive {
    $running = Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe'" | Where-Object { $_.CommandLine -like "*keepalive.sh*" }
    if (-not $running) {
        Start-Process -FilePath "wsl.exe" -ArgumentList @("-d", $Distro, "--", "/usr/lib/wsl/keepalive.sh") -WindowStyle Hidden | Out-Null
    }
}

switch ($Command.ToLower()) {
    "install" {
        if (-not $Package) { throw "Use -Package to point at the bilskirnir.wsl file." }
        if (-not (Test-Path $Package)) { throw "Package not found: $Package" }
        Invoke-Wsl @("--install", "--from-file", (Resolve-Path $Package).Path, "--name", $Distro)
        Write-Host "The distribution is registered. Its first start creates the Linux user and asks for the Bilskirnir admin password."
        Invoke-Wsl @("-d", $Distro)
    }
    "start" {
        if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
        Start-KeepAlive
        Invoke-Wsl @("-d", $Distro, "--", "bilskirnir", "start")
        Write-Host "Dashboard: http://localhost:$Port"
    }
    "stop" {
        & wsl.exe -d $Distro -- bilskirnir stop
        & wsl.exe --terminate $Distro
        Write-Host "Bilskirnir stopped."
    }
    "status" {
        if (Test-Registered) { & wsl.exe -d $Distro -- bilskirnir status } else { Write-Host "The $Distro distribution is not registered." }
    }
    "open" {
        Start-Process "http://localhost:$Port"
    }
    "autostart" {
        switch ($Option.ToLower()) {
            "enable" {
                $action = New-ScheduledTaskAction -Execute "wsl.exe" -Argument "-d $Distro -- /usr/lib/wsl/keepalive.sh"
                $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
                $settings = New-ScheduledTaskSettingsSet -Hidden -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
                Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Force | Out-Null
                Write-Host "Bilskirnir will keep running after you sign in to Windows."
            }
            "disable" {
                Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
                Write-Host "Auto-start removed."
            }
            default { throw "Use 'autostart enable' or 'autostart disable'." }
        }
    }
    "uninstall" {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        if (Test-Registered) {
            Write-Host "This removes the $Distro distribution with its Docker images, desktops and volumes."
            $answer = Read-Host "Type 'remove' to continue"
            if ($answer -eq "remove") { Invoke-Wsl @("--unregister", $Distro) } else { Write-Host "Kept the distribution." }
        }
    }
    default {
        Get-Help $MyInvocation.MyCommand.Path -Detailed | Out-String | Write-Host
    }
}
