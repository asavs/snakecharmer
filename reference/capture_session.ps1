<#
.SYNOPSIS
    Guided USB capture of Synapse writing button binds to a mouse's on-board memory.

.DESCRIPTION
    Starts USBPcapCMD on every USB root hub, walks you through one Synapse change
    at a time, and writes markers.json with each step's time window. Afterwards
    keymap_diff.py slices the captures by those windows. See docs/ONBOARD-KEYMAP.md.

    This script never talks to the mouse. It only records what Synapse sends.
    The raw .pcap files contain ALL USB traffic during the session, including
    your keyboard, so keep them local and share report.md instead.

.EXAMPLE
    .\capture_session.ps1 -Mouse V3
#>
param(
    [Parameter(Mandatory)][ValidateSet('V3', 'Elite')][string]$Mouse,
    [string]$OutDir = (Join-Path $HOME 'snakecharmer-captures'),
    [string]$USBPcapCMD = 'C:\Program Files\USBPcap\USBPcapCMD.exe'
)

$ErrorActionPreference = 'Stop'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw 'Run this from an Administrator PowerShell: USBPcap needs it to open the root hubs.' }
if (-not (Test-Path $USBPcapCMD)) { throw "USBPcapCMD not found at $USBPcapCMD. Install it: winget install desowin.USBPcap (then reboot)." }

# Absolute targets, not "change X to Y", so the same list works whatever the
# mouse currently has bound. The order isolates one variable per step:
# 01->02 the key code, 02->03 letter vs digit, 03->04 a modifier,
# 01 vs 05 the button id, then 06-08 put everything back to default.
$steps = @(
    @{ id = '00'; label = 'IDLE: Synapse open on the mouse''s button page. Touch nothing.'; idle = $true; seconds = 15 }
    @{ id = '01'; label = 'Set the BACK thumb button to the keyboard key 1' }
    @{ id = '02'; label = 'Set the BACK thumb button to the keyboard key 2' }
    @{ id = '03'; label = 'Set the BACK thumb button to the keyboard key A' }
    @{ id = '04'; label = 'Set the BACK thumb button to Shift + A' }
    @{ id = '05'; label = 'Set the FORWARD thumb button to the keyboard key 1' }
    @{ id = '06'; label = 'Set the FORWARD thumb button back to its default' }
    @{ id = '07'; label = 'Set the WHEEL CLICK back to its default' }
    @{ id = '08'; label = 'Set the BACK thumb button back to its default' }
    @{ id = '09'; label = 'IDLE: touch nothing.'; idle = $true; seconds = 8 }
)

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$session = Join-Path $OutDir "$Mouse-$stamp"
New-Item -ItemType Directory -Force $session | Out-Null

function Now { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0 }

# --- find root hubs -----------------------------------------------------------
$psi = New-Object Diagnostics.ProcessStartInfo $USBPcapCMD, '--extcap-interfaces'
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$p = [Diagnostics.Process]::Start($psi)
$listing = $p.StandardOutput.ReadToEnd()
$p.WaitForExit()
$hubs = [regex]::Matches($listing, 'value=(\\\\\.\\USBPcap\d+)') | ForEach-Object { $_.Groups[1].Value }
if (-not $hubs) { $hubs = 1..6 | ForEach-Object { "\\.\USBPcap$_" } }

# --- start one capture per hub -------------------------------------------------
$captures = @()
foreach ($hub in $hubs) {
    $name = $hub.Split('\')[-1]
    $file = Join-Path $session "$name.pcap"
    $proc = Start-Process $USBPcapCMD -ArgumentList "-d `"$hub`" -o `"$file`" -A" -PassThru -WindowStyle Hidden
    $captures += [pscustomobject]@{ Hub = $name; Proc = $proc; File = $file }
}
Start-Sleep -Seconds 3
$captures = @($captures | Where-Object { -not $_.Proc.HasExited })
if (-not $captures) { throw 'No USBPcap capture started. Was the PC rebooted after installing USBPcap?' }
Write-Host "Capturing on: $(($captures | ForEach-Object Hub) -join ', ')" -ForegroundColor DarkGray

$markers = [ordered]@{ mouse = $Mouse; started = (Get-Date).ToString('o'); steps = @() }
function Save-Markers { $markers | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $session 'markers.json') -Encoding utf8 }

try {
    Write-Host ''
    Write-Host "Session for the $Mouse. Open Synapse on the mouse's customize / button page now." -ForegroundColor Cyan
    Write-Host 'After each change, make sure Synapse has saved it to the mouse''s on-board memory.'
    Write-Host 'Press Enter when a step is done. To leave a note instead (e.g. "Synapse had no Shift option"), type it, then Enter.'
    Read-Host 'Ready? Press Enter to start' | Out-Null

    foreach ($s in $steps) {
        Write-Host ''
        Write-Host "[$($s.id)] $($s.label)" -ForegroundColor Yellow
        $start = Now
        $note = ''
        if ($s.idle) {
            for ($i = $s.seconds; $i -gt 0; $i--) { Write-Host -NoNewline "`r  $i s "; Start-Sleep -Seconds 1 }
            Write-Host "`r  done  "
        } else {
            $note = Read-Host '  Enter when done (or type a note)'
            Start-Sleep -Milliseconds 1500  # let Synapse's trailing writes land in this step
        }
        $entry = [ordered]@{ id = $s.id; label = $s.label; start = $start; end = (Now) }
        if ($s.idle) { $entry.idle = $true }
        if ($note) { $entry.note = $note }
        $markers.steps += $entry
        Save-Markers
    }
} finally {
    Start-Sleep -Seconds 2
    $captures | ForEach-Object { if (-not $_.Proc.HasExited) { Stop-Process -Id $_.Proc.Id -Force } }
    Save-Markers
}

Write-Host ''
Write-Host "Saved to $session" -ForegroundColor Green
Write-Host 'Next: quit Synapse completely (tray icon -> Exit), then check the buttons with:'
Write-Host "  .\button_listener.ps1"
$diff = Join-Path $PSScriptRoot 'keymap_diff.py'
if (Get-Command python -ErrorAction SilentlyContinue) {
    python -I $diff $session
}
