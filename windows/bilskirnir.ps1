# Bilskirnir's Windows helper: installs the WSL distribution, then starts, stops, updates, moves,
# resizes and removes it. It runs straight from a release, without saving it:
#
#   irm https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1 | iex
#
# 'irm | iex' runs this text in the caller's scope, so everything lives inside Invoke-Bilskirnir and
# only that function stays behind in the PowerShell window. Keep the file ASCII, since Windows
# PowerShell reads a script without a byte order mark in the ANSI code page, and never call 'exit',
# which would close the window that ran it.
function Invoke-Bilskirnir {
    param(
        [Parameter(Position = 0)][string]$Command = "",
        [Parameter(Position = 1)][string]$Option = "",
        [string]$Package = "",
        [string]$Location = "",
        [string]$VhdSize = "",
        [string]$Distro = "Bilskirnir",
        [int]$Port = 0
    )
    $ErrorActionPreference = "Stop"
    $taskName = "Bilskirnir keep-alive"
    $helperUrl = "https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1"
    $packageUrl = "https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.wsl"
    $usage = @"
Usage: bilskirnir.ps1 [command] [options]

Without a command it offers to install Bilskirnir, or lists what it can do with an installed one.

  install                             Download the latest package, register it and run its first-start setup
      -Location D:\WSL\Bilskirnir     Keep the distribution's disk in this folder
      -VhdSize 200GB                  Set the disk's maximum size (1 TB unless given)
      -Package .\bilskirnir.wsl       Install this package file instead of downloading one
  start                               Boot the distribution, keep it running and start the manager
  stop                                Stop the manager and shut the distribution down
  status                              Show the manager state
  open                                Open the dashboard in the browser
  update                              Move the installation to the newest release
  move D:\WSL\Bilskirnir              Move the distribution's disk to another folder
  resize 200GB                        Change the disk's maximum size
  autostart enable|disable            Start Bilskirnir at Windows sign-in through a scheduled task
  uninstall                           Remove the auto-start task and unregister the distribution

  -Distro <name>                      Work on a distribution installed under another name
  -Port <port>                        Use this dashboard port instead of the one in .env

A command runs without saving the helper too:
  & ([scriptblock]::Create((irm $helperUrl))) move D:\WSL\Bilskirnir

WSL stops a distribution shortly after its last Windows-started process ends, so 'start' and the
auto-start task keep a hidden keep-alive process open while Docker and the manager run inside.

WSL lets go of a stopped distribution's disk only when its virtual machine shuts down, so 'move'
and 'resize' shut WSL down, asking first when other distributions are running, and then start
Bilskirnir again.
"@

    function Invoke-Wsl { param([string[]]$Arguments) & wsl.exe @Arguments; if ($LASTEXITCODE -ne 0) { throw "wsl.exe exited with code $LASTEXITCODE" } }
    function Test-Registered { $list = (& wsl.exe --list --quiet) -join "`n"; return ($list -replace "`0", "") -match "(?m)^$Distro\s*$" }
    # A list that fails counts as nothing running.
    function Get-Running {
        $list = & wsl.exe --list --running --quiet
        if ($LASTEXITCODE -ne 0) { return @() }
        return @($list | ForEach-Object { ($_ -replace "`0", "").Trim() } | Where-Object { $_ })
    }
    function Get-DiskFolder {
        $keys = Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss -ErrorAction SilentlyContinue
        return ($keys | ForEach-Object { Get-ItemProperty $_.PSPath } | Where-Object { $_.DistributionName -eq $Distro }).BasePath
    }
    # wsl.exe resolves a relative path against its own working folder, so it gets a full one.
    function Resolve-Folder { param([string]$Path) return $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path) }
    # The port is a setting of the installation; -Port overrides what its .env says.
    function Get-Port {
        if ($Port) { return $Port }
        $value = (& wsl.exe -d $Distro -- sh -c "sed -n 's/^PORT=//p' /opt/bilskirnir/.env 2>/dev/null | tail -n 1") -join ""
        $value = ($value -replace "`0", "").Trim('"', "'", " ")
        if ($value -match '^[0-9]+$') { return [int]$value }
        return 8080
    }
    # Matches this distribution's keep-alive only, as started here or by the auto-start task.
    function Start-KeepAlive {
        $running = Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe'" | Where-Object { $_.CommandLine -like "*-d $Distro -- /usr/lib/wsl/keepalive.sh*" }
        if (-not $running) {
            Start-Process -FilePath "wsl.exe" -ArgumentList @("-d", $Distro, "--", "/usr/lib/wsl/keepalive.sh") -WindowStyle Hidden | Out-Null
        }
    }
    function Start-Bilskirnir {
        Start-KeepAlive
        Invoke-Wsl @("-d", $Distro, "--", "bilskirnir", "start")
        Write-Host "Dashboard: http://localhost:$(Get-Port)"
    }
    # A terminated distribution's disk stays attached to the WSL virtual machine, and WSL refuses to
    # move or resize it until the machine shuts down, which stops every other distribution too.
    function Invoke-DiskChange {
        param([string[]]$Arguments, [string]$Message)
        if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
        $running = Get-Running
        $others = @($running | Where-Object { $_ -ne $Distro })
        if ($others) {
            Write-Host "This shuts WSL down, which also stops $($others -join ', ')."
            if ((Read-Host "Type 'yes' to continue") -ne "yes") { Write-Host "Nothing was changed."; return }
        }
        if ($running -contains $Distro) { & wsl.exe -d $Distro -- bilskirnir stop }
        Invoke-Wsl @("--shutdown")
        Write-Host $Message
        try { Invoke-Wsl $Arguments } finally { Start-Bilskirnir }
    }

    # 'irm | iex' cannot pass a command, so without one the helper asks.
    if (-not $Command) {
        if (-not (Test-Registered)) {
            Write-Host "Bilskirnir is not installed yet. Its disk will hold every image, container and volume."
            $Location = Read-Host "Folder for the disk, such as D:\WSL\Bilskirnir (Enter for the WSL default)"
            $VhdSize = Read-Host "Maximum disk size, such as 200GB (Enter for 1 TB)"
            $Command = "install"
        } else {
            $state = if ((Get-Running) -contains $Distro) { "running" } else { "stopped" }
            $folder = Get-DiskFolder
            if ($folder) { Write-Host "Bilskirnir is $state, with its disk in $folder." } else { Write-Host "Bilskirnir is $state." }
            $autostart = [bool](Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)
            $choices = @(
                @("start", "Start it and keep it running"),
                @("stop", "Stop it"),
                @("status", "Show the manager state"),
                @("open", "Open the dashboard"),
                @("update", "Update it to the newest release"),
                @("move", "Move its disk to another folder"),
                @("resize", "Change its disk's maximum size"),
                @("autostart", $(if ($autostart) { "Stop starting it at Windows sign-in" } else { "Start it at every Windows sign-in" })),
                @("uninstall", "Remove it with its images, containers and volumes")
            )
            for ($i = 0; $i -lt $choices.Count; $i++) { Write-Host ("  {0}  {1}" -f ($i + 1), $choices[$i][1]) }
            $answer = Read-Host "Number (Enter to leave)"
            if (-not $answer) { return }
            $number = 0
            if (-not [int]::TryParse($answer, [ref]$number) -or $number -lt 1 -or $number -gt $choices.Count) { throw "Choose a number from 1 to $($choices.Count)." }
            $Command = $choices[$number - 1][0]
            if ($Command -eq "move") { $Option = Read-Host "New folder for the disk, such as D:\WSL\Bilskirnir" }
            if ($Command -eq "resize") { $Option = Read-Host "New maximum size, such as 200GB" }
            if ($Command -eq "autostart") { $Option = if ($autostart) { "disable" } else { "enable" } }
        }
    }

    switch ($Command.ToLower()) {
        "install" {
            if (Test-Registered) { throw "The $Distro distribution is already installed; 'update' moves it to the newest release." }
            $file = $Package
            if (-not $file) {
                $file = Join-Path $env:TEMP "bilskirnir-$PID.wsl"
                Write-Host "Downloading $packageUrl"
                & curl.exe -fL --progress-bar -o $file $packageUrl
                if ($LASTEXITCODE -ne 0) { Remove-Item $file -ErrorAction SilentlyContinue; throw "The download failed: curl.exe exited with code $LASTEXITCODE." }
            } elseif (-not (Test-Path $file)) { throw "Package not found: $Package" }
            # The first start runs below, after the message; WSL would otherwise launch it as well.
            $arguments = @("--install", "--from-file", (Resolve-Path $file).Path, "--name", $Distro, "--no-launch")
            if ($Location) { $arguments += @("--location", (Resolve-Folder $Location)) }
            if ($VhdSize) { $arguments += @("--vhd-size", $VhdSize) }
            try { Invoke-Wsl $arguments } finally { if (-not $Package) { Remove-Item $file -ErrorAction SilentlyContinue } }
            Write-Host "The distribution is registered. Its first start asks for the username and password used by Linux and the dashboard."
            Invoke-Wsl @("-d", $Distro)
            Write-Host "Run the helper again to start Bilskirnir, keep it running or move its disk."
        }
        "start" {
            if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
            Start-Bilskirnir
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
            Start-Process "http://localhost:$(Get-Port)"
        }
        "update" {
            if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
            Invoke-Wsl @("-d", $Distro, "--", "bilskirnir", "update")
        }
        "move" {
            if (-not $Option) { throw "Name the folder to move the disk to, for example 'move D:\WSL\Bilskirnir'." }
            $folder = Resolve-Folder $Option
            Invoke-DiskChange @("--manage", $Distro, "--move", $folder) "Moving the disk to $folder. Copying a large disk to another drive takes a while."
        }
        "resize" {
            if (-not $Option) { throw "Give the disk's new maximum size, for example 'resize 200GB'." }
            Invoke-DiskChange @("--manage", $Distro, "--resize", $Option) "Setting the disk's maximum size to $Option."
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
            Write-Host $usage
        }
    }
}

Invoke-Bilskirnir @args
