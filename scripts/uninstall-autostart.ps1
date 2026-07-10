# Removes the Anti-Synapse daemon autostart for the current user.
# Deletes the Startup-folder launcher. Does not stop a running daemon or touch
# any config/build artifacts.
$ErrorActionPreference = 'Stop'

$startup = [Environment]::GetFolderPath('Startup')
$vbs     = Join-Path $startup 'Anti-Synapse.vbs'
if (Test-Path $vbs) {
    Remove-Item $vbs -Force
    Write-Host "Removed autostart: $vbs"
} else {
    Write-Host "No Anti-Synapse autostart present."
}
