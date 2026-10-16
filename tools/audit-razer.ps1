# Read-only inventory of Razer/Synapse integrations. Never emits profile values,
# account tokens, device instance ids, or hardware serial numbers.
# PnPUtil label parsing below requires English Windows, as on the audited PC.
param([Parameter(Mandatory)][string]$OutputPath)
$ErrorActionPreference = 'Stop'
$auditErrors = [Collections.Generic.List[string]]::new()
function Probe([string]$Name, [scriptblock]$Action) {
    Write-Host "Auditing $Name"
    try { @(& $Action) } catch { $auditErrors.Add("${Name}: $($_.Exception.Message)"); @() }
}
$match = 'Razer|Synapse|Chroma|RzControl|RzVirtual|RzDev|RzCommon'
$uninstall = Probe 'uninstall' {
    foreach ($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall')) {
        Get-ItemProperty -Path "$root\*" -ErrorAction SilentlyContinue |
            Where-Object { $_.DisplayName -match $match -or $_.Publisher -match 'Razer' } |
            Select-Object @{n='Key';e={$_.PSPath}},DisplayName,DisplayVersion,Publisher,
                InstallLocation,UninstallString,QuietUninstallString
    }
}
$services = Probe 'services' {
    Get-CimInstance Win32_Service | Where-Object {
        "$($_.Name) $($_.DisplayName) $($_.PathName)" -match $match
    } | Select-Object Name,DisplayName,State,StartMode,PathName
}
$drivers = Probe 'system drivers' {
    Get-CimInstance Win32_SystemDriver | Where-Object {
        "$($_.Name) $($_.DisplayName) $($_.PathName)" -match $match
    } | Select-Object Name,DisplayName,State,StartMode,PathName
}
$packages = Probe 'driver store' {
    $driverText = (& pnputil.exe /enum-drivers) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'pnputil enumeration failed' }
    foreach ($block in ($driverText -split '(?:\r?\n){2,}')) {
        if ($block -notmatch $match) { continue }
        $fields = @{}
        foreach ($line in ($block -split '\r?\n')) {
            if ($line -match '^([^:]+):\s*(.+)$') { $fields[$Matches[1].Trim()]=$Matches[2].Trim() }
        }
        if ($fields['Published Name'] -notmatch '^oem\d+\.inf$' -or -not $fields['Original Name'] -or
            -not $fields['Provider Name']) { throw 'Cannot parse driver package labels; English Windows is required.' }
        [pscustomobject]@{Driver=$fields['Published Name'];OriginalFileName=$fields['Original Name'];
            ProviderName=$fields['Provider Name'];ClassName=$fields['Class Name'];
            Version=$fields['Driver Version'];Signer=$fields['Signer Name']}
    }
}
$devices = Probe 'PnP devices' {
    $present = @(Get-PnpDevice -PresentOnly | Select-Object -ExpandProperty InstanceId)
    Get-PnpDevice | Where-Object {
        $_.InstanceId -match 'VID_1532' -or $_.FriendlyName -match $match
    } | ForEach-Object {
        $dev = $_
        $p = @{}
        $properties = @(Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName @(
            'DEVPKEY_Device_DriverInfPath','DEVPKEY_Device_DriverProvider',
            'DEVPKEY_Device_Service','DEVPKEY_Device_UpperFilters','DEVPKEY_Device_LowerFilters') -ErrorAction SilentlyContinue)
        foreach ($v in $properties) { $p[$v.KeyName] = $v.Data }
        $model = if ($dev.InstanceId -match '(?i)VID_1532&PID_([0-9a-f]{4})') { "1532:$($Matches[1])" } else { 'virtual/other' }
        [pscustomobject]@{Model=$model;Name=$dev.FriendlyName;Class=$dev.Class;Status=$dev.Status;Present=($dev.InstanceId -in $present);
            Inf=$p['DEVPKEY_Device_DriverInfPath'];Provider=$p['DEVPKEY_Device_DriverProvider'];
            Service=$p['DEVPKEY_Device_Service'];UpperFilters=$p['DEVPKEY_Device_UpperFilters'];
            LowerFilters=$p['DEVPKEY_Device_LowerFilters']}
    }
}
$tasks = Probe 'scheduled tasks' {
    Get-ScheduledTask | Where-Object {
        "$($_.TaskPath) $($_.TaskName) $($_.Actions.Execute)" -match $match
    } | Select-Object TaskPath,TaskName,State,@{n='Executables';e={@($_.Actions.Execute)}}
}
$startup = Probe 'startup registry' {
    foreach ($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion')) {
        foreach ($leaf in @('Run','RunOnce')) {
            $key = "$root\$leaf"
            $item = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
            if ($item) { $item.PSObject.Properties | Where-Object {
                $_.Name -notmatch '^PS' -and "$($_.Name) $($_.Value)" -match $match
            } | ForEach-Object { [pscustomobject]@{Key=$key;Name=$_.Name;Command=$_.Value} } }
        }
    }
}
$filters = Probe 'class filters' {
    $root = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class'
    $keys = @(Get-ChildItem -LiteralPath $root)
    $keys += @($keys | ForEach-Object { Get-ChildItem -LiteralPath $_.PSPath -ErrorAction SilentlyContinue })
    foreach ($key in $keys) {
        $v = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
        foreach ($name in @('UpperFilters','LowerFilters')) {
            if (($v.$name -join ' ') -match $match) {
                [pscustomobject]@{Key=$key.Name;Value=$name;Filters=@($v.$name)}
            }
        }
    }
}
$registry = Probe 'vendor registry roots' {
    $softwareRoots = @('HKLM:\SOFTWARE','HKLM:\SOFTWARE\WOW6432Node','HKCU:\SOFTWARE')
    $softwareRoots += @(Get-ChildItem -LiteralPath Registry::HKEY_USERS |
        Where-Object { $_.PSChildName -notmatch '_Classes$' } |
        ForEach-Object { Join-Path $_.PSPath 'Software' })
    foreach ($root in $softwareRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        Get-ChildItem -LiteralPath $root | Where-Object { $_.PSChildName -match $match } |
            Select-Object @{n='Key';e={$_.Name}},SubKeyCount,ValueCount
    }
}
$appx = Probe 'AppX' {
    Get-AppxPackage -AllUsers | Where-Object { "$($_.Name) $($_.Publisher)" -match $match } |
        Select-Object Name,PackageFullName,InstallLocation
}
$firewall = Probe 'firewall registrations' {
    Get-NetFirewallRule | Where-Object { "$($_.DisplayName) $($_.Group)" -match $match } |
        ForEach-Object { $r=$_; [pscustomobject]@{Name=$r.Name;DisplayName=$r.DisplayName;
            Enabled=$r.Enabled.ToString();Program=@($r | Get-NetFirewallApplicationFilter | Select-Object -ExpandProperty Program)} }
}
$wmi = Probe 'WMI persistence' {
    Get-CimInstance -Namespace root/subscription -ClassName CommandLineEventConsumer |
        Where-Object { "$($_.Name) $($_.ExecutablePath) $($_.CommandLineTemplate)" -match $match } |
        Select-Object Name,ExecutablePath
    Get-CimInstance -Namespace root/subscription -ClassName ActiveScriptEventConsumer |
        Where-Object { "$($_.Name) $($_.ScriptFileName) $($_.ScriptText)" -match $match } |
        Select-Object Name,ScriptFileName
}
$processes = Probe 'processes' {
    Get-CimInstance Win32_Process | Where-Object {
        $_.Name -match $match -or
        $_.ExecutablePath -match '^C:\\(?:Program Files(?: \(x86\))?\\Razer\\|Windows\\Installer\\Razer\\)'
    } | Select-Object Name,ProcessId,ExecutablePath
}
$systemFiles = Probe 'Razer-named system files' {
    foreach ($root in @('C:\Windows\System32','C:\Windows\SysWOW64','C:\Windows\System32\drivers')) {
        Get-ChildItem -LiteralPath $root -File -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^(Razer|Rz).+\.(dll|exe|sys)$' } |
            ForEach-Object { [pscustomobject]@{Path=$_.FullName;Company=$_.VersionInfo.CompanyName;Bytes=$_.Length} }
    }
}
$comKeys = Probe 'owned COM and device-class registrations' {
    $keys = @('HKLM:\SOFTWARE\Classes\CLSID\{0EDEAF3C-D36E-4E7E-9467-900B977DC4FF}',
        'HKLM:\SOFTWARE\WOW6432Node\Classes\CLSID\{90b626f9-3e0d-efca-4b8d-16ddf2b1f1f4}',
        'HKLM:\SOFTWARE\Classes\AppID\{90b626f9-3e0d-efca-4b8d-16ddf2b1f1f4}',
        'HKLM:\SOFTWARE\WOW6432Node\Classes\AppID\{90b626f9-3e0d-efca-4b8d-16ddf2b1f1f4}',
        'HKLM:\SOFTWARE\Classes\TypeLib\{F24A9E29-FC07-4277-9BBE-8315A7735BF0}',
        'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{1750F915-5639-497C-966C-3A65ACECFCB6}')
    foreach ($root in @('HKLM:\SOFTWARE\Classes','HKLM:\SOFTWARE\WOW6432Node\Classes')) {
        $keys += "$root\AppID\{9C877A5A-7FE0-4E2B-9735-60420DE27084}"
        foreach ($iid in @('{BFE24D59-6568-4179-8AE5-D9D53869A3E3}','{C6237515-DD5F-4ACC-B6F0-F45569ADF965}')) { $keys += "$root\Interface\$iid" }
    }
    foreach ($key in $keys) { if (Test-Path -LiteralPath $key) { $key } }
}
$roots = @('C:\Program Files\Razer','C:\Program Files (x86)\Razer',
    'C:\Program Files (x86)\Razer Chroma SDK','C:\ProgramData\Razer',
    'C:\ProgramData\Razer Central','C:\Windows\Installer\Razer',
    (Join-Path $env:USERPROFILE 'AppData\Local\ToastNotificationManagerCompat\Apps\90B626F9-3E0D-EFCA-4B8D-16DDF2B1F1F4'))
# Per-profile roots (every folder under C:\Users plus the service profiles) are added below.
$profiles = @(Get-ChildItem -LiteralPath 'C:\Users' -Directory -Force |
    Where-Object { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) } |
    Select-Object -ExpandProperty FullName)
$profiles += @('C:\Windows\System32\config\systemprofile','C:\Windows\SysWOW64\config\systemprofile',
    'C:\Windows\ServiceProfiles\LocalService','C:\Windows\ServiceProfiles\NetworkService')
foreach ($profile in $profiles) {
    foreach ($leaf in @('AppData\Local\Razer','AppData\Local\RazerAppEngine',
        'AppData\Roaming\Razer','AppData\Roaming\RazerAppEngine','AppData\Roaming\Synapse3')) {
        $roots += Join-Path $profile $leaf
    }
}
$roots = @($roots | Select-Object -Unique)
$paths = Probe 'application paths' {
    foreach ($root in $roots) { if (Test-Path -LiteralPath $root) {
        $files = @(Get-ChildItem -LiteralPath $root -File -Recurse -Force -ErrorAction SilentlyContinue)
        [pscustomobject]@{Path=$root;Files=$files.Count;Bytes=($files|Measure-Object Length -Sum).Sum;
            Children=@(Get-ChildItem -LiteralPath $root -Force | Select-Object -ExpandProperty Name)}
    } }
}
$shortcuts = Probe 'shortcuts and temp top-level names' {
    foreach ($root in @('C:\ProgramData\Microsoft\Windows\Start Menu\Programs',
        (Join-Path $env:USERPROFILE 'AppData\Roaming\Microsoft\Windows\Start Menu\Programs'),
        (Join-Path $env:USERPROFILE 'AppData\Local\Temp'),'C:\Windows\Temp')) {
        if (Test-Path -LiteralPath $root) {
            Get-ChildItem -LiteralPath $root -Force -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match $match } | Select-Object FullName,Mode
        }
    }
}
$startupFolders = Probe 'startup folders' {
    $folders = @('C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup')
    foreach ($profile in $profiles) {
        $folders += Join-Path $profile 'AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup'
    }
    foreach ($folder in $folders) {
        if (Test-Path -LiteralPath $folder) {
            Get-ChildItem -LiteralPath $folder -Force -ErrorAction Stop |
                Where-Object Name -match $match | Select-Object FullName,Mode
        }
    }
}
$pending = Probe 'pending Razer file operations' {
    $key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager'
    $values = @((Get-ItemProperty -LiteralPath $key -Name PendingFileRenameOperations -ErrorAction SilentlyContinue).PendingFileRenameOperations)
    for ($i=0; $i -lt $values.Count; $i+=2) {
        $source = [string]$values[$i]
        $destination = if ($i+1 -lt $values.Count) { [string]$values[$i+1] } else { '' }
        if ("$source $destination" -match '(?i)\\Razer\\|\\Rz(?:Common|Dev_[0-9a-f]+)\.sys') {
            [pscustomobject]@{Source=$source;Destination=$destination;Deletion=([string]::IsNullOrEmpty($destination))}
        }
    }
}
$result = [ordered]@{Timestamp=(Get-Date).ToString('o');Uninstall=@($uninstall);
    Services=@($services);SystemDrivers=@($drivers);DriverPackages=@($packages);
    Devices=@($devices);Tasks=@($tasks);Startup=@($startup);ClassFilters=@($filters);
    RegistryRoots=@($registry);Appx=@($appx);Firewall=@($firewall);WmiConsumers=@($wmi);
    Processes=@($processes);SystemFiles=@($systemFiles);OwnedRegistrationKeys=@($comKeys);
    Paths=@($paths);NamedEntries=@($shortcuts);StartupFolders=@($startupFolders);
    PendingRazerFileOperations=@($pending);Errors=@($auditErrors)}
# Keep the account name and SID out of the saved inventory.
$json = $result | ConvertTo-Json -Depth 8
$json = [regex]::Replace($json, [regex]::Escape($env:USERPROFILE.Replace('\','\\')), '%USERPROFILE%', 'IgnoreCase')
$json = $json -replace 'S-1-5-21-\d+-\d+-\d+-\d+', '<user SID>'
$json | Out-File -Encoding utf8 -LiteralPath $OutputPath
$result.GetEnumerator() | Where-Object { $_.Key -ne 'Timestamp' } | ForEach-Object {
    '{0}: {1}' -f $_.Key,@($_.Value).Count
}
