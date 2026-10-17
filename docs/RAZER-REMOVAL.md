# Removing Razer Synapse 4 from Windows

Synapse 4 installs far more than an app: two Windows services, kernel filter drivers on
every Razer device it has seen, a scheduled task, startup entries, firewall rules, COM
registrations and about 1.5–2 GB of files. Its own uninstaller removes the app, the
services and most driver packages, and leaves the rest behind.

This page lists what Synapse installs, what the uninstaller leaves, and how to remove the
residue with the scripts in [`tools/`](../tools). It is based on two removals from the same
PC: 2026-10-07 (DeathAdder Elite attached) and 2026-10-16 (DeathAdder V3, after a
reinstall). Snakecharmer itself needs none of this: it talks to the mouse over standard
Windows HID.

## What Synapse 4 installs

| Kind | What was found |
|---|---|
| App | `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Razer Synapse` (4.0.827); uninstaller `C:\Windows\Installer\Razer\Installer2\App\RazerInstaller.exe /uninstall true`; AppEngine (Electron) at `C:\Program Files\Razer\RazerAppEngine` |
| Services | `Razer Elevation Service` (`C:\Program Files\Razer\razer_elevation_service`), `Razer Game Manager Service 3` (`C:\Program Files (x86)\Razer\Razer Services\GMS3`) |
| Kernel services | `RzCommon`, plus one `RzDev_<pid>` per Razer USB product ID seen (`RzDev_005c` Elite, `RzDev_00b2` V3, `RzDev_0a24` a keyboard); `.sys` files in `C:\Windows\System32\drivers` |
| Driver packages | `rzcommonu.inf` (shared control device); per device: `rz<pid>dev.inf` (co-installer), `rzdevu_<pid>_dkm.inf` (HID lower filter), `_kbd.inf` and `_mou.inf` (keyboard/mouse upper filters), `_vcon.inf` (virtual consumer-control device). Published as `oemNN.inf` |
| Device class | `HKLM\SYSTEM\CurrentControlSet\Control\Class\{1750F915-5639-497C-966C-3A65ACECFCB6}` ("Razer Device") |
| Scheduled task | `\Razer Upgrade Elevation Service`, running `RazerAppEngine\app-<version>\CommonDLL\RzPowerTool.exe` |
| Run / RunOnce | `HKCU\...\Run` value `RazerAppEngine`; `HKLM\SOFTWARE\WOW6432Node\...\RunOnce` value `CleanUp RzS2S3Wizard` |
| Firewall rules | `RazerAppEngine`, `RazerAppEngineUpgrade` (rule GUIDs change on reinstall) |
| COM | Elevation service: CLSID `{0EDEAF3C-D36E-4E7E-9467-900B977DC4FF}`, AppID `{9C877A5A-7FE0-4E2B-9735-60420DE27084}`, TypeLib `{F24A9E29-FC07-4277-9BBE-8315A7735BF0}`, interfaces `{BFE24D59-6568-4179-8AE5-D9D53869A3E3}` and `{C6237515-DD5F-4ACC-B6F0-F45569ADF965}`. Installer toast activator: CLSID/AppID `{90b626f9-3e0d-efca-4b8d-16ddf2b1f1f4}` |
| Toast AUMID | `{F38BF404-1D43-42F2-9305-67DE0B28FC23}/Installer/Razer/Installer2/App/RazerInstaller.exe` under `HKCU\SOFTWARE\Classes\AppUserModelId` and `...\Notifications\Settings`; icon in `%USERPROFILE%\AppData\Local\ToastNotificationManagerCompat\Apps\90B626F9-...` |
| File roots | `C:\Program Files\Razer`, `C:\Program Files (x86)\Razer`, `C:\ProgramData\Razer`, `C:\Windows\Installer\Razer`, `%USERPROFILE%\AppData\Local\Razer`, `C:\Windows\SysWOW64\config\systemprofile\AppData\Local\Razer`, Start Menu `Programs\Razer` |
| System files | `C:\Windows\System32\RazerS2S3CoinstallerEx.dll`, `RazerS3CoinstallerEx.dll`; Chroma SDK `System32\RzChromatic64.dll`, `SysWOW64\RzChromatic.dll` |
| Registry roots | `HKLM\SOFTWARE\Razer`, `HKLM\SOFTWARE\RAZERWUD`, `HKLM\SOFTWARE\WOW6432Node\Razer`, `HKCU\SOFTWARE\Razer`, `HKU\.DEFAULT\Software\Razer` |
| Temp pictures | `%TEMP%\RazerTempPicture_<n>_<n>.png` and `%TEMP%\RazerTempPicture<n>.png` |

The filter drivers sit in the input stack of each Razer mouse and keyboard interface, not
in the class-wide `UpperFilters`/`LowerFilters`, so a clean class key does not mean a
clean stack. Microsoft's `mouhid`, `kbdhid`, `HidUsb`, `mshidkmdf` and `usbccgp` are
Windows components and are never removed.

## What the uninstaller leaves behind

After the vendor uninstaller finished, the audits still found:

- the two or three co-installer driver packages (`rz<pid>dev.inf`) and every kernel
  service registration (`RzCommon`, `RzDev_*`), stopped
- the `.sys` driver files, some owned by TrustedInstaller
- 270–330 MB in the file roots above, including the per-user and system-profile caches
- all vendor registry roots and every COM registration, still pointing at the deleted
  elevation service and installer
- the scheduled task and the RunOnce value (2026-10-16)
- the installer's toast activator, AUMID and icon folder
- the co-installer DLLs in `System32`, the Start Menu folder and the temp pictures
- non-present device records still bound to Razer drivers and filters, and the
  Razer Device class

On 2026-10-07 two other local profiles also held `AppData\Local\Razer` folders (one with
Synapse 3 logs), and the Chroma SDK DLLs `RzChromatic64.dll` and `RzChromatic.dll` were present.

## Removal

Run everything from an elevated PowerShell in the repository. Each removal script is a dry
run unless you pass `-Apply`; the dry run prints and records what it would remove.

Windows can download the Razer driver and installer again the next time it sets up a Razer
device. Read [Windows puts it back](#windows-puts-it-back) first: if you want to stop that,
change the setting before step 3.

1. **Uninstall Synapse** with its registered uninstaller (Settings → Apps, or the
   `UninstallString` above). The residue and toast scripts refuse to run while Synapse is
   still registered.
2. **Audit.** [`audit-razer.ps1`](../tools/audit-razer.ps1) is read-only and writes a JSON
   inventory; [`audit-razer-supplement.ps1`](../tools/audit-razer-supplement.ps1) adds COM
   server paths, Winlogon/AppInit, Active Setup, IFEO, App Paths, SharedDLLs, DriverStore
   folders and credential target-name counts.
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\audit-razer.ps1 -OutputPath .\before.json
   ```
3. **Residue.** [`remove-razer-residue.ps1`](../tools/remove-razer-residue.ps1) removes the
   services, driver packages, kernel service registrations, Run/RunOnce values, firewall
   rules, task, registry roots, COM registrations, file roots (including
   `AppData\Local\Razer` in every local profile), co-installer and Chroma SDK DLLs,
   orphan `.sys` files and temp pictures. Each target is re-checked for Razer ownership
   first. It exits non-zero if any step failed.
   ```powershell
   .\tools\remove-razer-residue.ps1 -EvidenceDirectory .\evidence          # dry run
   .\tools\remove-razer-residue.ps1 -EvidenceDirectory .\evidence -Apply
   ```
4. **Non-present devices.** [`remove-razer-ghosts.ps1`](../tools/remove-razer-ghosts.ps1)
   removes non-present device records whose driver, service or filters are Razer's, then the
   Razer Device class once it is empty. Present devices are skipped and re-checked before
   each removal. Evidence keeps `VID_1532&PID_xxxx&MI_xx` and drops the instance suffix.
5. **Toast registration.** [`remove-razer-toast.ps1`](../tools/remove-razer-toast.ps1)
   verifies that the activator, AppID, AUMID and icon folder all point at each other, then
   removes them. Shared notification data stays.
6. **Audit again**, then reboot and audit a third time (see below).

Warnings:

- **Other Razer software shares all of this.** Cortex, THX Spatial Audio, Chroma apps,
  Razer keyboard and headset software, and Blade laptop utilities use the same folders,
  registry roots, drivers and Chroma SDK. Removing the Chroma SDK DLLs breaks Chroma
  lighting in every other app that uses it. The residue script refuses to run if any
  installed program other than Synapse/AppEngine lists Razer as publisher; don't work
  around that by hand.
- **OEM numbers differ per machine** and change on reinstall (`oem2.inf` was
  `rzcommonu.inf` on 2026-10-07 and `rz005cdev.inf` on 2026-10-16). The script matches
  packages by original INF name and Razer provider, re-checks each one immediately before
  `pnputil /delete-driver <oemNN.inf> /uninstall`, and stops on any unknown Razer INF. It
  never uses `/force` or `/reboot`.
- **TrustedInstaller-owned driver files.** `RzCommon.sys` and `RzDev_00b2.sys` were owned
  by TrustedInstaller without a delete right for Administrators. The residue script
  handles this itself, only for a file whose company is Razer, whose service is gone and
  whose driver is not loaded: `takeown /F <file> /A`, then
  `icacls <file> /grant *S-1-5-32-544:F`, then delete. It never changes a directory's
  owner. If a driver is still loaded, the script stops on that item; reboot and run it again.
- **`DXImageTransform.Microsoft.Chroma` is Windows.** A name search for "Chroma" matches
  `DXImageTransform.Microsoft.Chroma` and `.Chroma.1`; their server is
  `C:\Windows\System32\Dxtmsft.dll`, signed by Microsoft. Leave them alone.
- **English Windows only.** The scripts parse `pnputil` (and the supplement parses
  `cmdkey`) text output by its English labels. On other display languages the audit reports
  a parse error and the residue script stops before removing any driver package.
- Two other kinds of "Razer" match should stay: the mouse's own hardware records (bound
  to Microsoft drivers) and Snakecharmer.

## Results

| Check | [2026-10-07, Elite](audits/razer-removal-20261007) | [2026-10-16, V3](audits/razer-removal-20261016) |
|---|---|---|
| Baseline ([before](audits/razer-removal-20261007/before.json) / [before](audits/razer-removal-20261016/before.json)) | | |
| Installed app / services / kernel services | 1 / 2 / 4 | 1 / 2 / 3 |
| Driver packages | 12 | 11 |
| Scheduled tasks / Run+RunOnce / firewall rules | 0 / 1 / 1 | 1 / 2 / 2 |
| Vendor registry roots | 4 | 7 |
| File roots (size) | 5 (1.51 GB) | 6 (2.03 GB) |
| After vendor uninstaller ([after-vendor](audits/razer-removal-20261007/after-vendor.json) / [after-vendor](audits/razer-removal-20261016/after-vendor.json)) | | |
| Driver packages / kernel services | 3 / 4 | 2 / 3 |
| Task / RunOnce | 0 / 0 | 1 / 1 |
| Vendor registry roots / file roots (size) | 4 / 5 (328 MB) | 7 / 6 (272 MB) |
| Removed by the scripts ([events](audits/razer-removal-20261007/removal-events.json) / [events](audits/razer-removal-20261016/removal-events.json)) | | |
| Driver packages (pnputil exit 0) | 3 | 2 |
| Non-present device records ([Elite](audits/razer-removal-20261007/ghost-device-removal.json) / [V3](audits/razer-removal-20261016/ghost-device-removal.json)) | 16 | 9 |
| TrustedInstaller-owned `.sys` files ([Elite](audits/razer-removal-20261007/protected-orphan-removal.json) / [V3](audits/razer-removal-20261016/protected-orphan-removal.json)) | 2 | 2 |
| Temp pictures | 18 | 10 |
| Toast registration ([discovery](audits/razer-removal-20261016/toast-registration-discovery.json), [removal](audits/razer-removal-20261016/toast-removal.json)) | not audited | removed |
| Final, before reboot ([final](audits/razer-removal-20261007/final-before-reboot.json) / [final](audits/razer-removal-20261016/final-before-reboot.json)) | | |
| Apps, services, kernel services, driver packages, filters, tasks, Run/RunOnce, firewall rules, registry roots, COM keys, file roots, Razer-named system files, audit errors | 0 | 0 |
| Present mouse interfaces on Microsoft drivers, status OK, no Razer filters | 11 of 11 | 11 of 11 |
| Microsoft Chroma name matches kept ([evidence](audits/razer-removal-20261016/microsoft-chroma-exclusion.json)) | — | 2 |
| Razer deletions queued for reboot | — | 14 |
| Mouse and keyboard checked by hand ([verification](audits/razer-removal-20261007/verification.json) / [verification](audits/razer-removal-20261016/verification.json)) | passed | passed |

The remaining device records (50 and 62) are the mice's own hardware entries on
Microsoft drivers.

## Reboot check

The uninstaller queues deletions of its own folders in `PendingFileRenameOperations`;
they finish at the next restart. Leave that shared queue alone. After rebooting, run both
audits again:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\audit-razer.ps1 -OutputPath .\after-reboot.json
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\audit-razer-supplement.ps1 -OutputPath .\supplemental-after-reboot.json
```

Expect zero in every vendor section, no pending Razer file operations, no errors, and
normal mouse and keyboard input.

## Windows puts it back

On 2026-10-17, a day after a clean removal and a reboot, plugging the DeathAdder Elite in
reinstalled part of Synapse. Nobody ran an installer:

1. Windows set the mouse up as a new device ("Install Related Drivers" in
   `C:\Windows\INF\setupapi.dev.log`) and downloaded Razer's driver package,
   `rz005cdev.inf`, from Windows Update.
2. That package's co-installer ran as part of driver setup. It put back
   `System32\RazerS2S3CoinstallerEx.dll`, `HKLM\SOFTWARE\RAZERWUD`, the
   `RzS2S3WizardPkg` and `RzS3WizardS2S3` folders under `Program Files (x86)\Razer`, the
   RunOnce value, and the installer under `C:\Windows\Installer\Razer`.
3. Six seconds later it launched RazerInstaller. The installer offered Synapse; nothing was
   installed because nobody accepted.

A reboot doesn't prevent this. It happens whenever Windows sets up a Razer device: the
first time you plug one in, on a USB port it hasn't used before, or after a Windows reset.
Snakecharmer doesn't need the Razer driver; the mice work on Windows' own HID driver.

You have two choices.

**Leave Windows as it is.** Driver downloads keep working for every other device. When the
Razer installer appears, close it, then run the residue script again; it removes
everything listed above.

**Stop Windows downloading device drivers and apps.** In Settings → System → About →
Advanced system settings → Hardware → Device Installation Settings, choose **No**. The
dialog writes these two values; setting them from an elevated PowerShell does the same:

```powershell
Set-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\DriverSearching' SearchOrderConfig 0 -Type DWord
New-Item 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Metadata' -Force | Out-Null
Set-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Metadata' PreventDeviceMetadataFromNetwork 1 -Type DWord
```

This stops the Razer installer coming back. It doesn't affect Windows updates, security
patches, Store apps, or the drivers your current devices already have. The cost: a new
device gets Windows' built-in driver instead of the manufacturer's. That's usually enough,
but a printer, webcam or controller that needs the vendor's driver then has to be
installed from the vendor's site, or you can switch the setting back to **Yes** while you
plug it in. Windows has no switch that blocks one vendor's drivers: blocking the mouse's
device ID would stop the mouse working, and blocking its driver class would block every
mouse.

The residue script prints which of the two your PC is set to when it starts.
