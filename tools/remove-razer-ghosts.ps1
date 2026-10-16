#Requires -RunAsAdministrator
# Removes non-present device records still bound to Razer drivers or filters,
# then the Razer-only device class once it has no instances. Present devices
# are never touched. Dry run by default; pass -Apply to remove.
# Evidence keeps the hardware ID (VID_1532&PID_xxxx&MI_xx) but never the
# device instance suffix.
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
$eventsFile = Join-Path $EvidenceDirectory $(if ($Apply) { 'ghost-device-removal.json' } else { 'ghost-device-plan.json' })
$events = [Collections.Generic.List[object]]::new()

function Redact-Id([string]$InstanceId) { $InstanceId -replace '\\[^\\]+$', '\<instance>' }
function Redact-Text([string]$Text,[string]$InstanceId) {
    # Case-insensitive: pnputil may print the ID in a different case.
    $Text = [regex]::Replace($Text, [regex]::Escape($InstanceId), (Redact-Id $InstanceId).Replace('$','$$'), 'IgnoreCase')
    $Text -replace '(?i)\b((?:USB|HID|RZCONTROL|RZVIRTUAL|RAZER|ROOT|SWD)\\[^\\\s]+)\\\S+', '$1\<instance>'
}
function Save { $events | ConvertTo-Json -Depth 5 | Out-File -Encoding utf8 -LiteralPath $eventsFile }

$present = @(Get-PnpDevice -PresentOnly | Select-Object -ExpandProperty InstanceId)
$devices = @(Get-PnpDevice | Where-Object { $_.InstanceId -match 'VID_1532' -or $_.FriendlyName -match 'Razer' })
foreach ($dev in $devices) {
    if ($dev.InstanceId -in $present) { continue }
    $props = @{}
    Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName @(
        'DEVPKEY_Device_DriverProvider','DEVPKEY_Device_DriverInfPath',
        'DEVPKEY_Device_Service','DEVPKEY_Device_UpperFilters','DEVPKEY_Device_LowerFilters') -ErrorAction SilentlyContinue |
        ForEach-Object { $props[$_.KeyName] = $_.Data }
    $owned = $props['DEVPKEY_Device_DriverProvider'] -match '^Razer' -or
        $props['DEVPKEY_Device_Service'] -match '^Rz(Common|Dev_)' -or
        ($props['DEVPKEY_Device_UpperFilters'] -join ' ') -match 'RzDev_' -or
        ($props['DEVPKEY_Device_LowerFilters'] -join ' ') -match 'RzDev_'
    if (-not $owned) { continue }
    $id = Redact-Id $dev.InstanceId
    $inf = $props['DEVPKEY_Device_DriverInfPath']
    if (-not $Apply) {
        $events.Add([pscustomobject]@{Device=$id;Name=$dev.FriendlyName;Class=$dev.Class;Inf=$inf;Result='would remove'})
        Save
        Write-Host "Would remove: $($dev.Class) / $id / $inf"
        continue
    }
    # Re-check presence right before removal in case the device was plugged in.
    if (@(Get-PnpDevice -PresentOnly -InstanceId $dev.InstanceId -ErrorAction SilentlyContinue).Count) {
        throw "A candidate device is now present ($id); stopping."
    }
    $out = Redact-Text ((& pnputil.exe /remove-device $dev.InstanceId) -join "`n") $dev.InstanceId
    $code = $LASTEXITCODE
    $events.Add([pscustomobject]@{Device=$id;Name=$dev.FriendlyName;Class=$dev.Class;Inf=$inf;ExitCode=$code;Result=$out})
    Save
    Write-Host "Removed: $($dev.Class) / $id / $inf; exit $code"
    if ($code -notin @(0,3010)) { throw 'pnputil /remove-device failed; stopping.' }
}

$classKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{1750F915-5639-497C-966C-3A65ACECFCB6}'
if (Test-Path -LiteralPath $classKey) {
    if ((Get-ItemProperty -LiteralPath $classKey).Class -ne 'Razer Device') { throw 'Device class {1750F915-...} is not the Razer Device class' }
    $remaining = @(Get-PnpDevice -Class 'Razer Device' -ErrorAction SilentlyContinue)
    if ($Apply) {
        if ($remaining.Count) { throw 'Razer Device class still has instances; not removing it' }
        Remove-Item -LiteralPath $classKey -Recurse -Force
        $events.Add([pscustomobject]@{Device=$classKey;Name='Razer Device class';Result='removed'})
    } else {
        $events.Add([pscustomobject]@{Device=$classKey;Name='Razer Device class';Result='would remove once it has no instances'})
    }
    Save
}
$verb = if ($Apply) { 'Removed' } else { 'Would remove' }
Write-Host "$verb $(@($events | Where-Object Class).Count) non-present Razer device records."
if (-not $Apply) { Write-Host 'Dry run complete. Run again with -Apply to remove them.' }
