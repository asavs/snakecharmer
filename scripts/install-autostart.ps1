# Installs the Anti-Synapse daemon autostart for the current user.
# Creates a hidden (windowless) launcher in the Startup folder that runs the
# release build of anti-synapse.exe at login. Idempotent; no admin required.
#
# Reversible with uninstall-autostart.ps1.
$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$exe  = Join-Path $root 'target\release\anti-synapse.exe'
if (-not (Test-Path $exe)) {
    Write-Warning "Release build not found at $exe. Run 'cargo build --release' first."
}

$startup = [Environment]::GetFolderPath('Startup')
$vbs     = Join-Path $startup 'Anti-Synapse.vbs'
# WScript.Shell.Run(cmd, 0, False): windowStyle 0 = hidden, so the console app
# launches with no visible window. Interim mechanism until the tray app (P3+)
# ships as a native windowless (windows-subsystem) exe.
$line = 'CreateObject("WScript.Shell").Run """' + $exe + '""", 0, False'
Set-Content -Path $vbs -Value $line -Encoding ASCII
Write-Host "Installed autostart:`n  $vbs`n  -> $exe"
