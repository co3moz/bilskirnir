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
  lan enable|disable|status           Let other devices reach the dashboard through Windows
  uninstall                           Remove the auto-start task and unregister the distribution

WSL stops a distribution shortly after its last Windows-started process ends, so 'start' and the
auto-start task keep a hidden keep-alive process open while Docker and the manager run inside.

'lan enable' opens the dashboard's port, or -Port, through Windows once BIND_ADDRESS in the
distribution's .env lets the manager listen beyond localhost. It asks for administrator rights and sets
up what the WSL networking mode needs: a Hyper-V firewall rule under mirrored networking, a port proxy
to the distribution with a Windows firewall rule under NAT. The distribution gets a new NAT address
whenever WSL restarts, and 'start' and the auto-start task point the port proxy at it.
#>
param(
    [Parameter(Position = 0)][string]$Command = "help",
    [Parameter(Position = 1)][string]$Option = "",
    [string]$Package = "",
    [string]$Distro = "Bilskirnir",
    [int]$Port = 0,
    [string]$Address = "",
    [string]$TaskUser = "",
    [string]$Log = ""
)
# -Address, -TaskUser and -Log are what 'lan' hands the copy of this script that runs as administrator.
$ErrorActionPreference = "Stop"
$taskName = "Bilskirnir keep-alive"
# Under NAT, a task with administrator rights points the port proxy at the address in this file, which
# 'start' and the keep-alive task write without them.
$lanTaskName = "Bilskirnir LAN"
$addressFile = Join-Path $env:LOCALAPPDATA "Bilskirnir\wsl-address"

function Invoke-Wsl { param([string[]]$Arguments) & wsl.exe @Arguments; if ($LASTEXITCODE -ne 0) { throw "wsl.exe exited with code $LASTEXITCODE" } }
function Test-Registered { $list = (& wsl.exe --list --quiet) -join "`n"; return ($list -replace "`0", "") -match "(?m)^$Distro\s*$" }
# The settings these commands read from the installation's .env, without the quotes the first start puts
# around some values. The secrets in the same file stay inside the distribution.
function Get-Settings {
    $settings = @{}
    $lines = & wsl.exe -d $Distro -- sh -c "grep -E '^(PORT|BIND_ADDRESS|AUTH_MODE|MCP_ENABLED|MCP_AUTH_MODE)=' /opt/bilskirnir/.env 2>/dev/null"
    foreach ($line in $lines) {
        if (($line -replace "`0", "") -match '^([A-Z_]+)=(.*)$') { $settings[$Matches[1]] = $Matches[2].Trim('"', "'", " ") }
    }
    return $settings
}
# The port is a setting of the installation; -Port overrides what its .env says.
function Get-Port {
    param([hashtable]$Settings)
    if ($Port) { return $Port }
    if (-not $Settings) { $Settings = Get-Settings }
    if ($Settings["PORT"] -match '^[0-9]+$') { return [int]$Settings["PORT"] }
    return 8080
}
function Test-Loopback { param([string]$Bind) return ($Bind -like "127.*" -or $Bind -eq "localhost") }
function Start-KeepAlive {
    $running = Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe'" | Where-Object { $_.CommandLine -like "*keepalive.sh*" }
    if (-not $running) {
        Start-Process -FilePath "wsl.exe" -ArgumentList @("-d", $Distro, "--", "/usr/lib/wsl/keepalive.sh") -WindowStyle Hidden | Out-Null
    }
}
# The auto-start task. With LAN access under NAT it first reads the distribution's new address, without
# administrator rights, and has the LAN task point the port proxy at it.
function Register-KeepAlive {
    $actions = @()
    if (Get-ScheduledTask -TaskName $lanTaskName -ErrorAction SilentlyContinue) {
        $file = $addressFile -replace "'", "''"
        $lookup = "for (`$i = 0; `$i -lt 30; `$i++) { `$line = (wsl.exe -d $Distro --exec ip -4 -o addr show dev eth0) -join ' '; " +
            "if (`$line -match 'inet ([0-9.]+)/') { Set-Content -LiteralPath '$file' -Value `$Matches[1]; Start-ScheduledTask -TaskName '$lanTaskName'; break }; Start-Sleep -Seconds 2 }"
        $actions += New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-NoProfile -NonInteractive -WindowStyle Hidden -Command `"$lookup`""
    }
    $actions += New-ScheduledTaskAction -Execute "wsl.exe" -Argument "-d $Distro -- /usr/lib/wsl/keepalive.sh"
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -Hidden -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $taskName -Action $actions -Trigger $trigger -Settings $settings -Force | Out-Null
}

# The WSL networking mode, a Windows setting shared by every distribution: nat, mirrored or another.
function Get-NetworkingMode { return ((& wsl.exe -d $Distro --exec wslinfo --networking-mode) -join "").Trim() }
# The distribution's address on the WSL network under NAT, which changes whenever WSL restarts.
function Get-DistroAddress {
    $line = (& wsl.exe -d $Distro --exec ip -4 -o addr show dev eth0) -join " "
    if ($line -match 'inet ([0-9.]+)/') { return $Matches[1] }
    throw "Could not read the address of the $Distro distribution."
}
function Save-DistroAddress {
    param([string]$Value)
    New-Item -ItemType Directory -Force -Path (Split-Path $addressFile) | Out-Null
    Set-Content -LiteralPath $addressFile -Value $Value -Encoding ASCII
}
# Where the port proxy of 'lan enable' sends a port, as netsh lists it, or nothing.
function Get-PortProxy {
    param([int]$ListenPort)
    foreach ($line in (& netsh.exe interface portproxy show v4tov4)) {
        if ($line -match "^\s*0\.0\.0\.0\s+$ListenPort\s+(\S+)\s+[0-9]+\s*$") { return $Matches[1] }
    }
}
# The port a firewall rule of 'lan enable' lets in: the Windows firewall's under NAT, the Hyper-V
# firewall's under mirrored networking.
function Get-RulePort {
    param([switch]$HyperV)
    if ($HyperV) {
        if (-not (Get-Command Get-NetFirewallHyperVRule -ErrorAction SilentlyContinue)) { return }
        $rule = Get-NetFirewallHyperVRule -Name Bilskirnir -ErrorAction SilentlyContinue
        if ($rule) { $first = @($rule.LocalPorts)[0] }
    } else {
        $rule = Get-NetFirewallRule -Name Bilskirnir -ErrorAction SilentlyContinue
        if ($rule) { $first = @(($rule | Get-NetFirewallPortFilter).LocalPort)[0] }
    }
    if ($rule -and $first -match '^[0-9]+$') { return [int]$first }
}
function Test-LanSetUp {
    param([int]$ListenPort)
    return [bool]((Get-RulePort) -or (Get-RulePort -HyperV) -or (Get-PortProxy $ListenPort) -or (Get-ScheduledTask -TaskName $lanTaskName -ErrorAction SilentlyContinue))
}
# The addresses other devices use for this computer: those of the connected adapters with a gateway.
function Get-WindowsAddresses {
    $addresses = @(Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway -and $_.NetAdapter.Status -eq "Up" } | ForEach-Object { $_.IPv4Address.IPAddress })
    if (-not $addresses) { $addresses = @("<this computer's address>") }
    return $addresses
}

function Test-Administrator {
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}
# A task with the highest run level gets administrator rights only for an account in the Administrators
# group, which a PowerShell without them still lists, as a group used for deny only.
function Test-AdministratorAccount { return [bool]((& whoami.exe /groups /fo csv /nh) -match '"S-1-5-32-544"') }

# The Windows side of LAN access, as administrator. It removes what an earlier 'lan enable' set up, then
# opens the port for nat or mirrored networking, or nothing for none. Under NAT it adds the port proxy to
# the distribution's address, the Windows firewall rule and, for an administrator's account, the LAN
# task. That task accepts only an IPv4 address from the address file, which the account can write
# without administrator rights.
function Set-LanAccess {
    param([string]$Mode, [int]$ListenPort, [string]$Target, [string]$User)
    $ports = @($ListenPort)
    $rule = Get-NetFirewallRule -Name Bilskirnir -ErrorAction SilentlyContinue
    if ($rule) { $ports += @(($rule | Get-NetFirewallPortFilter).LocalPort) }
    foreach ($old in $ports | Select-Object -Unique) { netsh.exe interface portproxy delete v4tov4 listenport=$old listenaddress=0.0.0.0 | Out-Null }
    Remove-NetFirewallRule -Name Bilskirnir -ErrorAction SilentlyContinue
    $hyperV = [bool](Get-Command New-NetFirewallHyperVRule -ErrorAction SilentlyContinue)
    if ($hyperV) { Remove-NetFirewallHyperVRule -Name Bilskirnir -ErrorAction SilentlyContinue }
    Unregister-ScheduledTask -TaskName $lanTaskName -Confirm:$false -ErrorAction SilentlyContinue
    if ($Mode -eq "mirrored") {
        if (-not $hyperV) { throw "Mirrored networking needs the Hyper-V firewall, which this Windows lacks." }
        New-NetFirewallHyperVRule -Name Bilskirnir -DisplayName Bilskirnir -Direction Inbound -VMCreatorId "{40E0AC32-46A5-438A-A0B2-2B479E8F2E90}" -Protocol TCP -LocalPorts $ListenPort | Out-Null
    } elseif ($Mode -eq "nat") {
        netsh.exe interface portproxy add v4tov4 listenport=$ListenPort listenaddress=0.0.0.0 connectport=$ListenPort connectaddress=$Target | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "netsh could not forward port $ListenPort to $Target." }
        New-NetFirewallRule -Name Bilskirnir -DisplayName Bilskirnir -Direction Inbound -Protocol TCP -LocalPort $ListenPort -Action Allow | Out-Null
        if ($User) {
            $file = $addressFile -replace "'", "''"
            $refresh = "`$address = Get-Content -LiteralPath '$file' -TotalCount 1; if (`$address -match '^[0-9]{1,3}(\.[0-9]{1,3}){3}`$') { " +
                "netsh.exe interface portproxy set v4tov4 listenport=$ListenPort listenaddress=0.0.0.0 connectport=$ListenPort connectaddress=`$address }"
            $action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-NoProfile -NonInteractive -WindowStyle Hidden -Command `"$refresh`""
            $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
            $settings = New-ScheduledTaskSettingsSet -Hidden -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Minutes 5)
            Register-ScheduledTask -TaskName $lanTaskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
        }
    }
}
# Points the port proxy at the distribution's new address, as administrator.
function Set-PortProxy {
    param([int]$ListenPort, [string]$Target)
    netsh.exe interface portproxy set v4tov4 listenport=$ListenPort listenaddress=0.0.0.0 connectport=$ListenPort connectaddress=$Target | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "netsh could not forward port $ListenPort to $Target." }
}
# One administrator step of 'lan': nat, mirrored or none set LAN access up for that mode or remove it,
# proxy points the port proxy at a new address. None of them runs anything from the distribution.
function Invoke-AdministratorStep {
    param([string]$Step, [int]$ListenPort, [string]$Target, [string]$User)
    if ($Step -eq "proxy") { Set-PortProxy $ListenPort $Target }
    elseif ($Step -in "nat", "mirrored", "none") { Set-LanAccess $Step $ListenPort $Target $User }
    else { throw "Unknown administrator step: $Step" }
}
# Runs an administrator step, through a UAC prompt unless this PowerShell already has the rights. The
# elevated copy of this script gets the step and its values on its command line, which the UAC prompt
# shows. It has a window of its own, so its error comes back in a file.
function Invoke-Elevated {
    param([string]$Step, [int]$ListenPort, [string]$Target = "", [string]$User = "")
    if (Test-Administrator) { Invoke-AdministratorStep $Step $ListenPort $Target $User; return }
    $log = [IO.Path]::GetTempFileName()
    $arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$PSCommandPath`" elevated $Step -Port $ListenPort -Log `"$log`""
    if ($Target) { $arguments += " -Address $Target" }
    if ($User) { $arguments += " -TaskUser `"$User`"" }
    try {
        $process = Start-Process -FilePath "powershell.exe" -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    } catch {
        Remove-Item -LiteralPath $log -ErrorAction SilentlyContinue
        throw "This step needs administrator rights, and the UAC prompt was declined."
    }
    $failure = Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $log -ErrorAction SilentlyContinue
    if ($failure) { throw $failure.Trim() }
    if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) { throw "The administrator step exited with code $($process.ExitCode)." }
}

# After WSL restarts, the port proxy of 'lan enable' sends the port to the distribution's old NAT address.
# The LAN task points it at the new one; without that task, UAC asks.
function Update-PortProxy {
    $lanPort = Get-RulePort
    if (-not $lanPort) { return }
    $target = Get-PortProxy $lanPort
    if (-not $target) { return }
    if ((Get-NetworkingMode) -ne "nat") {
        Write-Warning "Port $lanPort still has the port proxy of NAT networking, which WSL no longer uses. Run 'lan enable' again."
        return
    }
    $address = Get-DistroAddress
    if ($target -eq $address) { return }
    Save-DistroAddress $address
    if (Get-ScheduledTask -TaskName $lanTaskName -ErrorAction SilentlyContinue) {
        try { Start-ScheduledTask -TaskName $lanTaskName } catch { }
        for ($i = 0; $i -lt 20 -and (Get-PortProxy $lanPort) -ne $address; $i++) { Start-Sleep -Milliseconds 500 }
    }
    if ((Get-PortProxy $lanPort) -ne $address) {
        Write-Host "$Distro has a new address, $address. Windows asks for administrator rights to forward port $lanPort to it."
        Invoke-Elevated "proxy" $lanPort $address
    }
    Write-Host "Port $lanPort reaches $Distro at its new address, $address."
}

function Enable-Lan {
    if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
    $settings = Get-Settings
    $lanPort = Get-Port $settings
    $bind = $settings["BIND_ADDRESS"]
    if (-not $bind) { $bind = "127.0.0.1" }
    if (Test-Loopback $bind) {
        throw "The manager listens on ${bind}:$lanPort, which only this computer reaches, whatever Windows allows. Set BIND_ADDRESS=0.0.0.0 with 'bilskirnir settings' in the distribution, run 'bilskirnir restart', then 'lan enable' again."
    }
    if ($settings["AUTH_MODE"] -eq "none") {
        Write-Warning "The dashboard has no password (AUTH_MODE=none), so any device that reaches it controls every container."
    }
    if ($settings["MCP_ENABLED"] -eq "true" -and $settings["MCP_AUTH_MODE"] -eq "none") {
        Write-Warning "Agents need no token (MCP_AUTH_MODE=none), so any device that reaches the agent endpoint controls every container."
    }
    $mode = Get-NetworkingMode
    $address = ""
    $taskUser = ""
    if ($mode -eq "nat") {
        $address = Get-DistroAddress
        Save-DistroAddress $address
        if (Test-AdministratorAccount) { $taskUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name }
    } elseif ($mode -ne "mirrored") {
        throw "WSL uses '$mode' networking; 'lan' handles NAT and mirrored networking only."
    }
    Write-Host "Opening port $lanPort for other devices under $mode networking. Windows asks for administrator rights."
    Invoke-Elevated $mode $lanPort $address $taskUser
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { Register-KeepAlive }
    if ($mode -eq "mirrored") {
        Write-Host "The Hyper-V firewall lets port $lanPort in."
    } else {
        Write-Host "Windows forwards port $lanPort to $Distro at $address, and its firewall lets the port in."
        if ($taskUser) {
            Write-Host "That address changes whenever WSL restarts; 'start' and auto-start then point the port proxy at the new one through the '$lanTaskName' task."
        } else {
            Write-Host "That address changes whenever WSL restarts; 'start' then asks for administrator rights to point the port proxy at the new one."
        }
    }
    foreach ($ip in Get-WindowsAddresses) { Write-Host "Other devices open http://${ip}:$lanPort" }
}

function Disable-Lan {
    $lanPort = if ($Port) { $Port } elseif (Test-Registered) { Get-Port } else { 8080 }
    if (-not (Test-LanSetUp $lanPort)) {
        Write-Host "Windows has nothing set up for other devices."
        return
    }
    Write-Host "Closing the way in for other devices. Windows asks for administrator rights."
    Invoke-Elevated "none" $lanPort
    Remove-Item -LiteralPath $addressFile -ErrorAction SilentlyContinue
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { Register-KeepAlive }
    Write-Host "Other devices no longer get through Windows to Bilskirnir. BIND_ADDRESS in .env is unchanged."
}

function Show-Lan {
    if (-not (Test-Registered)) {
        Write-Host "The $Distro distribution is not registered."
        return
    }
    $settings = Get-Settings
    $lanPort = Get-Port $settings
    $bind = $settings["BIND_ADDRESS"]
    if (-not $bind) { $bind = "127.0.0.1" }
    $mode = Get-NetworkingMode
    $rulePort = Get-RulePort
    $hyperVPort = Get-RulePort -HyperV
    Write-Host "Manager:        listens on ${bind}:$lanPort"
    Write-Host "WSL networking: $mode"
    if ($hyperVPort) { Write-Host "Hyper-V rule:   lets port $hyperVPort in" }
    if ($rulePort) { Write-Host "Firewall rule:  lets port $rulePort in" }
    $open = 0
    $problem = ""
    if (Test-Loopback $bind) {
        $problem = "the manager listens on this computer only; set BIND_ADDRESS=0.0.0.0 with 'bilskirnir settings'"
    } elseif ($mode -eq "mirrored") {
        if ($hyperVPort) { $open = $hyperVPort } else { $problem = "the Hyper-V firewall keeps them out; run 'lan enable'" }
    } elseif ($mode -eq "nat") {
        $proxyPort = if ($rulePort) { $rulePort } else { $lanPort }
        $target = Get-PortProxy $proxyPort
        $address = Get-DistroAddress
        if ($target) { Write-Host "Port proxy:     port $proxyPort to ${target}:$proxyPort" }
        if (Get-ScheduledTask -TaskName $lanTaskName -ErrorAction SilentlyContinue) { Write-Host "LAN task:       '$lanTaskName' follows the address after WSL restarts" }
        if (-not $target) {
            $problem = "Windows does not forward the port; run 'lan enable'"
        } elseif (-not $rulePort) {
            $problem = "the Windows firewall has no rule for the port; run 'lan enable'"
        } elseif ($target -ne $address) {
            $problem = "the port proxy points at an old address, $Distro now has $address; run 'start'"
        } else {
            $open = $rulePort
        }
    } else {
        $problem = "'lan' handles NAT and mirrored networking only"
    }
    if ($open) {
        foreach ($ip in Get-WindowsAddresses) { Write-Host "Other devices:  http://${ip}:$open" }
    } else {
        Write-Host "Other devices:  cannot get in, $problem."
    }
}

switch ($Command.ToLower()) {
    "install" {
        if (-not $Package) { throw "Use -Package to point at the bilskirnir.wsl file." }
        if (-not (Test-Path $Package)) { throw "Package not found: $Package" }
        Invoke-Wsl @("--install", "--from-file", (Resolve-Path $Package).Path, "--name", $Distro)
        Write-Host "The distribution is registered. Its first start asks for the username and password used by Linux and the dashboard."
        Invoke-Wsl @("-d", $Distro)
    }
    "start" {
        if (-not (Test-Registered)) { throw "The $Distro distribution is not registered. Run 'install' first." }
        Start-KeepAlive
        Invoke-Wsl @("-d", $Distro, "--", "bilskirnir", "start")
        try { Update-PortProxy } catch { Write-Warning "Other devices may not reach Bilskirnir: $($_.Exception.Message)" }
        Write-Host "Dashboard: http://localhost:$(Get-Port)"
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
    "autostart" {
        switch ($Option.ToLower()) {
            "enable" {
                Register-KeepAlive
                Write-Host "Bilskirnir will keep running after you sign in to Windows."
            }
            "disable" {
                Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
                Write-Host "Auto-start removed."
            }
            default { throw "Use 'autostart enable' or 'autostart disable'." }
        }
    }
    "lan" {
        switch ($Option.ToLower()) {
            "enable" { Enable-Lan }
            "disable" { Disable-Lan }
            "status" { Show-Lan }
            default { throw "Use 'lan enable', 'lan disable' or 'lan status'." }
        }
    }
    "elevated" {
        # The copy of this script that Invoke-Elevated starts as administrator.
        try {
            Invoke-AdministratorStep $Option $Port $Address $TaskUser
        } catch {
            if ($Log) { Set-Content -LiteralPath $Log -Value $_ }
            exit 1
        }
    }
    "uninstall" {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        if (Test-Registered) {
            Write-Host "This removes the $Distro distribution with its Docker images, desktops and volumes."
            $answer = Read-Host "Type 'remove' to continue"
            if ($answer -eq "remove") {
                $lanPort = Get-Port
                if (Test-LanSetUp $lanPort) {
                    Write-Host "Removing the way in for other devices too. Windows asks for administrator rights."
                    try {
                        Invoke-Elevated "none" $lanPort
                        Remove-Item -LiteralPath $addressFile -ErrorAction SilentlyContinue
                    } catch {
                        Write-Warning "$($_.Exception.Message) Run 'lan disable' later to remove the firewall rules and the port proxy."
                    }
                }
                Invoke-Wsl @("--unregister", $Distro)
            } else {
                Write-Host "Kept the distribution."
            }
        }
    }
    default {
        Get-Help $MyInvocation.MyCommand.Path -Detailed | Out-String | Write-Host
    }
}
