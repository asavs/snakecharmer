# Read-only targeted checks supplementing audit-razer.ps1. Values from profiles,
# account stores and credentials are never emitted. No offline hives are loaded.
param([Parameter(Mandatory)][string]$OutputPath)
$ErrorActionPreference='Stop'
$errors=[Collections.Generic.List[string]]::new()
function Probe([string]$Name,[scriptblock]$Action){
    try { @(& $Action) } catch { $errors.Add("${Name}: $($_.Exception.Message)"); @() }
}
$match='Razer|Synapse|Chroma|RzDev|RzCommon'
$namedClasses=Probe 'named classes' {
    foreach($root in @('HKLM:\SOFTWARE\Classes','HKLM:\SOFTWARE\WOW6432Node\Classes','HKCU:\SOFTWARE\Classes')){
        if(Test-Path -LiteralPath $root){
            Get-ChildItem -LiteralPath $root | Where-Object PSChildName -match $match | Select-Object -ExpandProperty Name
        }
    }
}
$systemReferences=Probe 'Winlogon and AppInit' {
    foreach($key in @('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon',
        'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Windows')){
        if(Test-Path -LiteralPath $key){
            $item=Get-ItemProperty -LiteralPath $key
            foreach($property in $item.PSObject.Properties){
                if($property.Name -notmatch '^PS' -and ($property.Value -join ' ') -match $match){
                    [pscustomobject]@{Key=$key;ValueName=$property.Name}
                }
            }
        }
    }
}
$activationReferences=Probe 'Active Setup IFEO and App Paths' {
    foreach($root in @('HKLM:\SOFTWARE\Microsoft\Active Setup\Installed Components',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Active Setup\Installed Components',
        'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Image File Execution Options',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths')){
        if(-not(Test-Path -LiteralPath $root)){continue}
        foreach($key in Get-ChildItem -LiteralPath $root){
            $item=Get-ItemProperty -LiteralPath $key.PSPath
            $matchesVendor=$key.PSChildName -match $match
            foreach($property in $item.PSObject.Properties){
                if($property.Name -notmatch '^PS' -and ($property.Value -join ' ') -match $match){$matchesVendor=$true}
            }
            if($matchesVendor){$key.Name}
        }
    }
}
$sharedDlls=Probe 'SharedDLL paths' {
    foreach($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\SharedDLLs')){
        if(Test-Path -LiteralPath $root){
            (Get-Item -LiteralPath $root).GetValueNames() | Where-Object {$_ -match '(?i)\\Razer\\|\\RzChromatic(?:64)?\.dll$'}
        }
    }
}
$comReferences=Probe 'COM server paths' {
    # Avoid repeated provider traversals across thousands of COM keys.
    foreach($location in @(
        @{Hive=[Microsoft.Win32.RegistryHive]::LocalMachine;View=[Microsoft.Win32.RegistryView]::Registry64},
        @{Hive=[Microsoft.Win32.RegistryHive]::LocalMachine;View=[Microsoft.Win32.RegistryView]::Registry32},
        @{Hive=[Microsoft.Win32.RegistryHive]::CurrentUser;View=[Microsoft.Win32.RegistryView]::Registry64})){
        $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($location.Hive,$location.View)
        try {
            $root=$base.OpenSubKey('SOFTWARE\Classes\CLSID',$false)
            if(-not $root){continue}
            try {
                foreach($id in $root.GetSubKeyNames()){
                    foreach($leaf in @('LocalServer32','InprocServer32')){
                        $serverKey=$root.OpenSubKey("$id\$leaf",$false)
                        if(-not $serverKey){continue}
                        try {
                            $server=[string]$serverKey.GetValue('')
                            if($server -match '(?i)\\Razer\\|\\RzChromatic(?:64)?\.dll'){
                                [pscustomobject]@{Key=$serverKey.Name;View=$location.View.ToString();ServerType=$leaf}
                            }
                        } finally {$serverKey.Dispose()}
                    }
                }
            } finally {$root.Dispose()}
        } finally {$base.Dispose()}
    }
}
$credentialTargets=Probe 'credential target names' {
    $lines=& cmdkey.exe /list
    if($LASTEXITCODE -ne 0){throw 'Credential target enumeration failed'}
    foreach($line in $lines){
        if($line -match '^\s*Target:\s*(.+)$' -and $Matches[1] -match $match){'matching target'}
    }
}
$notificationReferences=Probe 'installer notification identities' {
    $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,[Microsoft.Win32.RegistryView]::Registry64)
    try {
        foreach($path in @('SOFTWARE\Classes\AppUserModelId','SOFTWARE\Microsoft\Windows\CurrentVersion\Notifications\Settings')){
            $key=$base.OpenSubKey($path,$false)
            if(-not $key){continue}
            try {
                foreach($name in $key.GetSubKeyNames()){
                    if($name -match 'Razer|Synapse'){[pscustomobject]@{Root=$path;Name=$name}}
                }
            } finally {$key.Dispose()}
        }
    } finally {$base.Dispose()}
}
$driverFolders=Probe 'known Razer DriverStore folders' {
    Get-ChildItem -LiteralPath 'C:\Windows\System32\DriverStore\FileRepository' -Directory |
        Where-Object Name -match '^(rz[0-9a-f]+dev|rzdevu_[0-9a-f]+_(dkm|kbd|mou|vcon)|rzcommonu)\.inf_' |
        Select-Object -ExpandProperty FullName
}
[ordered]@{
    Timestamp=(Get-Date).ToString('o')
    NamedClassOrProtocolRegistrations=@($namedClasses)
    WinlogonOrAppInitReferences=@($systemReferences)
    ActiveSetupIFEOOrAppPathsReferences=@($activationReferences)
    SharedDLLReferences=@($sharedDlls)
    COMServerReferences=@($comReferences)
    RazerNamedCredentialTargetCount=@($credentialTargets).Count
    InstallerNotificationReferences=@($notificationReferences)
    KnownRazerDriverStoreDirectories=@($driverFolders)
    Errors=@($errors)
} | ConvertTo-Json -Depth 5 | Out-File -LiteralPath $OutputPath -Encoding utf8
Write-Host "Supplemental audit complete; $($errors.Count) errors."
