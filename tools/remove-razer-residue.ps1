#Requires -RunAsAdministrator
# Removes Razer Synapse 4 residue left after the vendor uninstaller. Dry run by
# default: prints and records what it would remove. Pass -Apply to remove.
# Every target is an exact allowlisted path, key, or name, and each is
# re-checked for Razer ownership before removal. Never touches firmware,
# Microsoft drivers, or the reboot queue, and never forces a reboot.
# PnPUtil output parsing assumes English Windows.
param(
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'

# Evidence directory first, so every later failure can be recorded.
try {
    $EvidenceDirectory = (New-Item -ItemType Directory -Force -Path $EvidenceDirectory).FullName
    $probe = Join-Path $EvidenceDirectory '.write-test'
    Set-Content -LiteralPath $probe -Value '' -Encoding utf8
    Remove-Item -LiteralPath $probe -Force
} catch {
    Write-Error "Evidence directory '$EvidenceDirectory' is not writable: $($_.Exception.Message)"
    exit 2
}
$mode = if ($Apply) { 'apply' } else { 'dry run' }
$eventsFile = Join-Path $EvidenceDirectory $(if ($Apply) { 'removal-events.json' } else { 'removal-plan.json' })
$errorsFile = Join-Path $EvidenceDirectory $(if ($Apply) { 'removal-errors.txt' } else { 'removal-plan-errors.txt' })
$events = [Collections.Generic.List[object]]::new()
$failures = [Collections.Generic.List[string]]::new()
$script:acted = $false

function Record([string]$Kind,[string]$Target,[string]$Result) {
    $events.Add([pscustomobject]@{Time=(Get-Date).ToString('o');Kind=$Kind;Target=$Target;Result=$Result})
    Write-Host "$Kind $Target : $Result"
    $events | ConvertTo-Json -Depth 5 | Out-File -Encoding utf8 -LiteralPath $eventsFile
}
# Gate for every destructive call: returns $true only with -Apply.
function Commit { $script:acted = $true; return [bool]$Apply }
function Attempt([string]$Kind,[string]$Target,[scriptblock]$Action) {
    $script:acted = $false
    try {
        & $Action
        $result = if (-not $script:acted) { 'absent' } elseif ($Apply) { 'removed' } else { 'would remove' }
        Record $Kind $Target $result
    } catch {
        $failures.Add("${Kind} ${Target}: $($_.Exception.Message)")
        Record $Kind $Target "error: $($_.Exception.Message)"
    }
}
function Redact([string]$Text) {
    # PnPUtil may print device instance IDs; keep the hardware ID, drop the instance.
    $Text -replace '(?i)\b((?:USB|HID|RZCONTROL|RZVIRTUAL|RAZER|ROOT)\\[^\\\s]+)\\\S+', '$1\<instance>'
}
function Assert-Owner([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }

try {
    Write-Host "Mode: $mode. Evidence: $EvidenceDirectory"

    # Refuse if other Razer software is installed: Cortex, THX, Chroma apps and
    # device software share these roots, drivers and the Chroma SDK DLLs.
    $others = @(foreach ($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall')) {
        Get-ItemProperty -Path "$root\*" -ErrorAction SilentlyContinue |
            Where-Object { $_.Publisher -match 'Razer' -and $_.DisplayName -notmatch '^(Razer Synapse|Razer ?AppEngine)\b' } |
            ForEach-Object { "$($_.DisplayName) $($_.DisplayVersion) [$($_.Publisher)]" }
    })
    if ($others.Count) {
        Write-Host 'Other Razer products are installed. They share the folders, registry keys and'
        Write-Host 'drivers this script removes, so it will not run:'
        $others | ForEach-Object { Write-Host "  $_" }
        exit 3
    }
    $uninstallKey = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Razer Synapse'
    if (Test-Path -LiteralPath $uninstallKey) { throw 'Synapse is still registered. Run its uninstaller first.' }
    if (@(Get-Process -Name RazerAppEngine -ErrorAction SilentlyContinue).Count) { throw 'RazerAppEngine is running. Close it first.' }

    # Application services. Kernel drivers are handled by PnPUtil below.
    foreach ($name in @('Razer Elevation Service','Razer Game Manager Service 3')) {
        Attempt 'service' $name {
            $key = "HKLM:\SYSTEM\CurrentControlSet\Services\$name"
            if (Test-Path -LiteralPath $key) {
                $image = (Get-ItemProperty -LiteralPath $key).ImagePath
                Assert-Owner ($image -match '^"?C:\\Program Files(?: \(x86\))?\\Razer\\') 'Service binary is not in a Razer folder'
                if (Commit) {
                    $svc = Get-Service -Name $name
                    if ($svc.Status -ne 'Stopped') { Stop-Service -Name $name; $svc.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(20)) }
                    $out = (& sc.exe delete $name) -join "`n"
                    if ($LASTEXITCODE -ne 0) { throw $out }
                }
            }
        }
    }

    # Driver packages. OEM numbers differ per machine and change on reinstall, so
    # match by original INF name and Razer provider, and re-check before each delete.
    $originalInfs = @('rzdevu_005c_vcon.inf','rzdevu_00b2_vcon.inf',
        'rzdevu_005c_dkm.inf','rzdevu_00b2_dkm.inf',
        'rzdevu_005c_kbd.inf','rzdevu_00b2_kbd.inf',
        'rzdevu_005c_mou.inf','rzdevu_00b2_mou.inf',
        'rz005cdev.inf','rz00b2dev.inf','rz0a24dev.inf','rzcommonu.inf')
    $driverText = (& pnputil.exe /enum-drivers) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'pnputil /enum-drivers failed' }
    if ($driverText -notmatch '(?m)^Published Name:') { throw 'Cannot parse pnputil output (English Windows required)' }
    $packages = [ordered]@{}
    foreach ($block in ($driverText -split '(?:\r?\n){2,}')) {
        if ($block -notmatch '(?m)^Provider Name:\s+Razer Inc\.?\s*$') { continue }
        if ($block -notmatch '(?m)^Original Name:\s+(\S+)\s*$') { throw 'Cannot parse pnputil output (English Windows required)' }
        $original = $Matches[1]
        if ($original -notin $originalInfs) { throw "Unreviewed Razer driver package: $original. Review it before continuing." }
        if ($block -notmatch '(?m)^Published Name:\s+(oem\d+\.inf)\s*$') { throw 'Cannot parse pnputil output (English Windows required)' }
        $packages[$Matches[1]] = $original
    }
    foreach ($entry in $packages.GetEnumerator()) {
        $target = "$($entry.Key) ($($entry.Value))"
        if (-not $Apply) { Record 'driver package' $target 'would remove'; continue }
        $text = (& pnputil.exe /enum-drivers) -join "`n"
        if ($LASTEXITCODE -ne 0) { throw 'pnputil /enum-drivers failed' }
        $block = @($text -split '(?:\r?\n){2,}' | Where-Object {
            $_ -match ('(?m)^Published Name:\s+'+[regex]::Escape($entry.Key)+'\s*$')
        })
        if (-not $block.Count) { Record 'driver package' $target 'absent'; continue }
        if ($block.Count -ne 1 -or $block[0] -notmatch ('(?m)^Original Name:\s+'+[regex]::Escape($entry.Value)+'\s*$') -or
            $block[0] -notmatch '(?m)^Provider Name:\s+Razer Inc\.?\s*$') { throw "Driver package changed: $($entry.Key)" }
        $out = Redact ((& pnputil.exe /delete-driver $entry.Key /uninstall) -join "`n")
        $code = $LASTEXITCODE
        Record 'driver package' $target "exit $code; $out"
        if ($code -notin @(0,3010)) { throw "pnputil could not remove $($entry.Key); stopping" }
    }

    # Orphan kernel service registrations. Never unload a running input driver.
    $kernelServices = @('RzCommon','RzDev_005c','RzDev_00b2','RzDev_0a24')
    foreach ($name in $kernelServices) {
        Attempt 'kernel service' $name {
            $key = "HKLM:\SYSTEM\CurrentControlSet\Services\$name"
            if (Test-Path -LiteralPath $key) {
                $image = (Get-ItemProperty -LiteralPath $key).ImagePath
                Assert-Owner ($image -match ('(?i)\\drivers\\'+[regex]::Escape($name)+'\.sys$')) 'Service image is not the Razer driver'
                $driver = Get-CimInstance Win32_SystemDriver | Where-Object Name -eq $name
                if ($driver.State -eq 'Running') { throw 'Driver is loaded; reboot, then run again' }
                if (Commit) { $out = (& sc.exe delete $name) -join "`n"; if ($LASTEXITCODE -ne 0) { throw $out } }
            }
        }
    }

    Attempt 'Run value' 'HKCU Run\RazerAppEngine' {
        $key = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
        $v = Get-ItemProperty -LiteralPath $key -Name RazerAppEngine -ErrorAction SilentlyContinue
        if ($v) {
            Assert-Owner ($v.RazerAppEngine -match '^"?C:\\Program Files\\Razer\\RazerAppEngine\\') 'Run value does not point to AppEngine'
            if (Commit) { Remove-ItemProperty -LiteralPath $key -Name RazerAppEngine }
        }
    }
    Attempt 'RunOnce value' 'HKLM WOW6432Node RunOnce\CleanUp RzS2S3Wizard' {
        $key = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce'
        $v = Get-ItemProperty -LiteralPath $key -Name 'CleanUp RzS2S3Wizard' -ErrorAction SilentlyContinue
        if ($v) {
            Assert-Owner ($v.'CleanUp RzS2S3Wizard' -ieq 'C:\Program Files (x86)\Razer\RzS3WizardS2S3\RzInstallerDeletionS2S3.vbs') 'RunOnce value points elsewhere'
            if (Commit) { Remove-ItemProperty -LiteralPath $key -Name 'CleanUp RzS2S3Wizard' }
        }
    }
    foreach ($rule in @(Get-NetFirewallRule | Where-Object DisplayName -in @('RazerAppEngine','RazerAppEngineUpgrade'))) {
        Attempt 'firewall rule' "$($rule.DisplayName) $($rule.Name)" {
            $program = @($rule | Get-NetFirewallApplicationFilter | Select-Object -ExpandProperty Program)
            Assert-Owner ($program.Count -eq 1 -and $program[0] -match '^C:\\Program Files\\Razer\\RazerAppEngine\\') 'Rule program is not AppEngine'
            if (Commit) { $rule | Remove-NetFirewallRule }
        }
    }
    Attempt 'scheduled task' '\Razer Upgrade Elevation Service' {
        $task = Get-ScheduledTask -TaskPath '\' -TaskName 'Razer Upgrade Elevation Service' -ErrorAction SilentlyContinue
        if ($task) {
            Assert-Owner ($task.Actions.Count -eq 1 -and $task.Actions[0].Execute -match '^"?C:\\Program Files\\Razer\\RazerAppEngine\\app-[0-9.]+\\CommonDLL\\RzPowerTool\.exe"?$') 'Task action is not RzPowerTool'
            if (Commit) { Unregister-ScheduledTask -InputObject $task -Confirm:$false }
        }
    }
    foreach ($key in @('HKLM:\SOFTWARE\Razer','HKLM:\SOFTWARE\RAZERWUD',
        'HKLM:\SOFTWARE\WOW6432Node\Razer','HKCU:\SOFTWARE\Razer',
        'Registry::HKEY_USERS\.DEFAULT\Software\Razer','Registry::HKEY_USERS\S-1-5-18\Software\Razer')) {
        Attempt 'registry root' $key { if (Test-Path -LiteralPath $key) { if (Commit) { Remove-Item -LiteralPath $key -Recurse -Force } } }
    }

    # COM registrations of the elevation service, each checked against its owner.
    $razerTypeLib = '{F24A9E29-FC07-4277-9BBE-8315A7735BF0}'
    $elevationExe = 'C:\Program Files\Razer\razer_elevation_service\razer_elevation_service.exe'
    foreach ($view in @('HKLM:\SOFTWARE\Classes','HKLM:\SOFTWARE\WOW6432Node\Classes')) {
        foreach ($iid in @('{BFE24D59-6568-4179-8AE5-D9D53869A3E3}','{C6237515-DD5F-4ACC-B6F0-F45569ADF965}')) {
            $key = "$view\Interface\$iid"
            Attempt 'COM interface' $key {
                if (Test-Path -LiteralPath $key) {
                    Assert-Owner ((Get-Item -LiteralPath "$key\TypeLib").GetValue('') -ieq $razerTypeLib) 'Interface type library is not Razer'
                    if (Commit) { Remove-Item -LiteralPath $key -Recurse -Force }
                }
            }
        }
        $key = "$view\AppID\{9C877A5A-7FE0-4E2B-9735-60420DE27084}"
        Attempt 'COM AppID' $key {
            if (Test-Path -LiteralPath $key) {
                Assert-Owner ((Get-ItemProperty -LiteralPath $key).LocalService -ieq 'Razer Elevation Service') 'AppID service is not Razer'
                if (Commit) { Remove-Item -LiteralPath $key -Recurse -Force }
            }
        }
    }
    $key = 'HKLM:\SOFTWARE\Classes\CLSID\{0EDEAF3C-D36E-4E7E-9467-900B977DC4FF}'
    Attempt 'COM class' $key {
        if (Test-Path -LiteralPath $key) {
            Assert-Owner ((Get-Item -LiteralPath "$key\LocalServer32").GetValue('') -ieq "`"$elevationExe`"") 'COM server is not the elevation service'
            if (Commit) { Remove-Item -LiteralPath $key -Recurse -Force }
        }
    }
    $key = "HKLM:\SOFTWARE\Classes\TypeLib\$razerTypeLib"
    Attempt 'COM type library' $key {
        if (Test-Path -LiteralPath $key) {
            Assert-Owner ((Get-Item -LiteralPath "$key\1.0\0\win64").GetValue('') -ieq $elevationExe) 'Type library is not the elevation service'
            if (Commit) { Remove-Item -LiteralPath $key -Recurse -Force }
        }
    }

    # Exact directory allowlist, including AppData\Local\Razer in every local profile.
    $roots = [Collections.Generic.List[string]]@('C:\Program Files\Razer','C:\Program Files (x86)\Razer',
        'C:\ProgramData\Razer','C:\Windows\Installer\Razer',
        'C:\Windows\SysWOW64\config\systemprofile\AppData\Local\Razer',
        'C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Razer',
        (Join-Path $env:USERPROFILE 'AppData\Local\Razer'))
    Get-ChildItem -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList' |
        ForEach-Object { [Environment]::ExpandEnvironmentVariables([string]$_.GetValue('ProfileImagePath')) } |
        Where-Object { $_ -like 'C:\Users\*' } |
        ForEach-Object { $roots.Add((Join-Path $_ 'AppData\Local\Razer')) }
    foreach ($root in @($roots | Select-Object -Unique)) {
        Attempt 'directory' $root {
            if (Test-Path -LiteralPath $root) {
                $item = Get-Item -LiteralPath $root -Force
                $resolved = [IO.Path]::GetFullPath($item.FullName).TrimEnd('\')
                Assert-Owner ($resolved -ieq $root.TrimEnd('\')) 'Resolved path is outside the allowlist'
                Assert-Owner (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Path is a reparse point'
                $links = @(Get-ChildItem -LiteralPath $resolved -Directory -Recurse -Force |
                    Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
                Assert-Owner ($links.Count -eq 0) 'Contains reparse-point directories; review by hand'
                if (Commit) { Remove-Item -LiteralPath $resolved -Recurse -Force }
            }
        }
    }

    # Co-installers and Chroma SDK DLLs. Removing the SDK DLLs breaks Chroma in
    # other apps, which is why other Razer products block this script above.
    foreach ($path in @('C:\Windows\System32\RazerS2S3CoinstallerEx.dll','C:\Windows\System32\RazerS3CoinstallerEx.dll',
        'C:\Windows\System32\RzChromatic64.dll','C:\Windows\SysWOW64\RzChromatic.dll')) {
        Attempt 'system file' $path {
            if (Test-Path -LiteralPath $path) {
                $item = Get-Item -LiteralPath $path
                Assert-Owner ($item.VersionInfo.CompanyName -match '^Razer') 'File company is not Razer'
                if (Commit) { Remove-Item -LiteralPath $item.FullName -Force }
            }
        }
    }

    # Orphan driver files. Windows leaves some owned by TrustedInstaller with no
    # delete right for Administrators. Only for a Razer-signed file whose service
    # is gone and whose driver is not loaded: take ownership of that one file,
    # grant Administrators full control, delete it.
    foreach ($name in $kernelServices) {
        $path = "C:\Windows\System32\drivers\$name.sys"
        Attempt 'driver file' $path {
            if (Test-Path -LiteralPath $path) {
                $driver = Get-CimInstance Win32_SystemDriver | Where-Object Name -eq $name
                if ($driver.State -eq 'Running') { throw 'Driver is loaded; reboot, then run again' }
                if (Test-Path -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$name") {
                    if ($Apply) { throw 'Driver service is still registered' }
                    # In a dry run the service removal above was only planned.
                }
                $item = Get-Item -LiteralPath $path
                Assert-Owner ($item.VersionInfo.CompanyName -match '^Razer') 'File company is not Razer'
                $owner = (Get-Acl -LiteralPath $path).Owner
                if (Commit) {
                    if ($owner -match 'TrustedInstaller') {
                        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
                        $out = (& takeown.exe /F $path /A) -join "`n"
                        if ($LASTEXITCODE -ne 0) { throw "takeown failed: $out" }
                        $out = (& icacls.exe $path /grant '*S-1-5-32-544:F') -join "`n"
                        if ($LASTEXITCODE -ne 0) { throw "icacls failed: $out" }
                        Record 'driver file ownership' $path "previous owner $owner; SHA256 $hash; ownership taken"
                    }
                    Remove-Item -LiteralPath $item.FullName -Force
                } elseif ($owner -match 'TrustedInstaller') {
                    Write-Host "  $path is owned by $owner; -Apply will take ownership of this file before deleting it"
                }
            }
        }
    }

    $temp = (Get-Item -LiteralPath $env:TEMP).FullName.TrimEnd('\')
    Get-ChildItem -LiteralPath $temp -File -Force | Where-Object { $_.Name -match '^RazerTempPicture(?:\d+|_\d+_\d+)\.png$' } |
        ForEach-Object { $file = $_; Attempt 'temp file' $file.FullName {
            Assert-Owner ($file.DirectoryName -ieq $temp -and -not ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Temp file path changed'
            if (Commit) { Remove-Item -LiteralPath $file.FullName -Force }
        } }
} catch {
    $failures.Add("stopped: $($_.Exception.Message)")
    Write-Host "Stopped: $($_.Exception.Message)"
}

$failures | Out-File -Encoding utf8 -LiteralPath $errorsFile
if ($failures.Count) {
    Write-Host "$($failures.Count) step(s) failed; see $errorsFile."
    exit 1
}
if (-not $Apply) { Write-Host 'Dry run complete. Run again with -Apply to remove the items above.' }
exit 0
