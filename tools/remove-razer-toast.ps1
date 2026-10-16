#Requires -RunAsAdministrator
# Removes the Razer Installer toast-notification registration the vendor
# uninstaller leaves behind: its COM activator (HKLM), AppID, current-user
# AppUserModelId and notification settings, and its icon cache folder. Each link
# is verified before anything is removed. Shared notification data is kept.
# Dry run by default; pass -Apply to remove.
param(
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
try {
    $EvidenceDirectory = (New-Item -ItemType Directory -Force -Path $EvidenceDirectory).FullName
} catch {
    Write-Error "Cannot create evidence directory '$EvidenceDirectory': $($_.Exception.Message)"
    exit 2
}
$id = '{90b626f9-3e0d-efca-4b8d-16ddf2b1f1f4}'
$aumid = '{F38BF404-1D43-42F2-9305-67DE0B28FC23}/Installer/Razer/Installer2/App/RazerInstaller.exe'
$classKey = "HKLM:\SOFTWARE\WOW6432Node\Classes\CLSID\$id"
$expectedServer = '"C:\Windows\Installer\Razer\Installer2\App\RazerInstaller.exe" -ToastActivated'
$iconRoot = Join-Path $env:USERPROFILE 'AppData\Local\ToastNotificationManagerCompat\Apps\90B626F9-3E0D-EFCA-4B8D-16DDF2B1F1F4'
$appIdKeys = @('HKLM:\SOFTWARE\Classes\AppID','HKLM:\SOFTWARE\WOW6432Node\Classes\AppID') | ForEach-Object { Join-Path $_ $id }
$events = [Collections.Generic.List[string]]::new()
$prefix = if ($Apply) { 'Removed' } else { 'Would remove' }

if (Test-Path 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Razer Synapse') {
    throw 'Synapse is still registered. Run its uninstaller first.'
}

# Registry key names with forward slashes are literal; use the registry API so
# the app identity is not parsed as a path.
$userBase = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,[Microsoft.Win32.RegistryView]::Registry64)
try {
    $appRoot = $userBase.OpenSubKey('SOFTWARE\Classes\AppUserModelId', [bool]$Apply)
    $app = if ($appRoot) { $appRoot.OpenSubKey($aumid, $false) } else { $null }

    if (-not (Test-Path -LiteralPath $classKey)) {
        if ($app) { throw "The toast activator $classKey is gone but the app identity remains; review it by hand." }
        Write-Host 'No Razer Installer toast registration found. Nothing to remove.'
        exit 0
    }
    if ((Get-Item -LiteralPath "$classKey\LocalServer32").GetValue('') -ine $expectedServer) { throw 'Toast activator does not point to RazerInstaller.exe -ToastActivated' }
    if ((Get-ItemProperty -LiteralPath $classKey).AppId -ine $id) { throw 'Toast activator AppID does not match' }
    foreach ($key in $appIdKeys) {
        if ((Test-Path -LiteralPath $key) -and (Get-ItemProperty -LiteralPath $key).RunAs -ine 'Interactive User') { throw "$key is not the installer AppID" }
    }
    if (-not $appRoot) {
        Write-Host 'HKCU\SOFTWARE\Classes\AppUserModelId does not exist; only the HKLM activator and AppID remain.'
    } elseif (-not $app) {
        Write-Host 'The Razer Installer app identity is not registered for this user; only the HKLM activator and AppID remain.'
    } else {
        try {
            if ($app.GetValue('CustomActivator') -ine $id -or $app.GetValue('DisplayName') -ine 'Razer Installer' -or
                $app.GetValue('IconUri') -ine "$iconRoot\Icon.png") { throw 'App identity does not match the Razer Installer activator and icon' }
        } finally { $app.Dispose() }
    }
    if (Test-Path -LiteralPath $iconRoot) {
        $item = Get-Item -LiteralPath $iconRoot -Force
        if ([IO.Path]::GetFullPath($item.FullName).TrimEnd('\') -ine $iconRoot -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Icon folder path mismatch' }
        if (@(Get-ChildItem -LiteralPath $iconRoot -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) { throw 'Icon folder contains links' }
    }

    # Everything verified. Remove (or report).
    if ($app) {
        if ($Apply) { $appRoot.DeleteSubKeyTree($aumid, $false) }
        $events.Add("$prefix HKCU\SOFTWARE\Classes\AppUserModelId\$aumid")
    }
    if ($appRoot) { $appRoot.Dispose() }
    $settings = $userBase.OpenSubKey('SOFTWARE\Microsoft\Windows\CurrentVersion\Notifications\Settings', [bool]$Apply)
    if ($settings) {
        try {
            $existing = $settings.OpenSubKey($aumid, $false)
            if ($existing) {
                $existing.Dispose()
                if ($Apply) { $settings.DeleteSubKeyTree($aumid, $false) }
                $events.Add("$prefix HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Notifications\Settings\$aumid")
            }
        } finally { $settings.Dispose() }
    }
} finally { $userBase.Dispose() }

foreach ($key in $appIdKeys) {
    if (Test-Path -LiteralPath $key) {
        if ($Apply) { Remove-Item -LiteralPath $key -Recurse -Force }
        $events.Add("$prefix $key")
    }
}
if ($Apply) { Remove-Item -LiteralPath $classKey -Recurse -Force }
$events.Add("$prefix $classKey")
if (Test-Path -LiteralPath $iconRoot) {
    if ($Apply) { Remove-Item -LiteralPath $iconRoot -Recurse -Force }
    $events.Add("$prefix $($iconRoot.Replace($env:USERPROFILE, '%USERPROFILE%'))")
}
$out = Join-Path $EvidenceDirectory $(if ($Apply) { 'toast-removal.json' } else { 'toast-plan.json' })
$events | ConvertTo-Json | Out-File -LiteralPath $out -Encoding utf8
$events
if (-not $Apply) { Write-Host 'Dry run complete. Run again with -Apply to remove the items above.' }
