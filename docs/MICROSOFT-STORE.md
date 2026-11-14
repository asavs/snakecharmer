# Microsoft Store package

Snakecharmer ships to the Microsoft Store as an MSIX package. The Store signs the
package, so this needs no code-signing certificate. Individual developer accounts are
free.

## What the package changes

The exes are the same release build. They check at run time whether they have package
identity (`platform::package::is_packaged`), and a packaged copy differs in these places:

| | Bare exe | Store package |
|---|---|---|
| Start at login | HKCU `Run` value, registered on first run | `windows.startupTask` in the manifest, enabled after first launch |
| Tray startup item | **Request start with Windows**: shows the request, not the effect | **Start with Windows**: shows and sets the startup task's real state |
| Start menu, search | none | **Snakecharmer** |
| `charmctl` | run from wherever you put it | on `PATH` from any terminal (app execution alias) |
| Uninstall | by hand, see the README | Settings > Apps > Installed apps |

Config and log stay in `%LOCALAPPDATA%\Snakecharmer` either way. The manifest excludes
that folder from MSIX file virtualization, so the documented path, Notepad, and a bare
`charmctl` all see the same files as the packaged daemon.

A packaged app's HKCU registry writes go to a private hive, so the packaged daemon never
reads or writes the `Run` value. If a bare-exe copy registered itself earlier, that
registration is still there; delete the `Snakecharmer` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` after moving to the Store copy. Leaving
it is harmless (the second copy exits on the single-instance lease) but it launches the
old exe.

## Files

- `packaging/msix/AppxManifest.xml`: manifest template.
- `packaging/msix/identity.toml`: package name and publisher. Placeholders until Partner
  Center assigns real values.
- `packaging/msix/Assets/`: icons, rendered from `site/favicon.svg` by
  `tools/render-msix-assets.py`. Rerun that only when the logo changes.
- `tools/build-msix.py`: builds the release exes and packs
  `target/msix/Snakecharmer_<version>_x64.msix`.
- `site/privacy.html`: privacy policy, published at
  <https://asavs.github.io/snakecharmer/privacy.html>.

## Build and test locally

```powershell
python tools/build-msix.py              # build + pack
python tools/build-msix.py --register   # also install the staged layout
```

`--register` installs the unsigned layout in place, which needs Developer Mode
(Settings > System > For developers). Quit any running Snakecharmer first; the release
build cannot replace a running exe.

Check after registering:

1. **Snakecharmer** appears in Start search and launches to the tray.
2. `daemon.log` shows `Packaged install: login startup is the package startup task`.
3. The tray shows **Start with Windows** checked. Unchecking and rechecking it logs
   `startup task On -> Off` and `Off -> On`, and Settings > Apps > Startup agrees.
4. `charmctl status` works from a new terminal.
5. With a mouse plugged in, the usual hardware check from `AGENTS.md`.

Remove the test install with `Get-AppxPackage Snakecharmer.Dev | Remove-AppxPackage`.
The registration points at `target\msix\layout`, so `cargo clean` breaks it.

## First Store submission

1. Sign in to [Partner Center](https://partner.microsoft.com/dashboard/apps-and-games/overview)
   with a personal Microsoft account and register as an individual developer.
2. **Apps and games > New product > MSIX or PWA app**, and reserve the name
   **Snakecharmer**.
3. **Product management > Product identity**: copy `Package/Identity/Name`,
   `Package/Identity/Publisher`, and `Package/Properties/PublisherDisplayName` into
   `packaging/msix/identity.toml`. Commit that change.
4. Run `python tools/build-msix.py` and upload `target/msix/Snakecharmer_<version>_x64.msix`
   under **Packages**.
5. **Properties**: category Utilities & tools. Privacy policy URL:
   `https://asavs.github.io/snakecharmer/privacy.html`.
6. **Submission options > Restricted capabilities**: explain the two the manifest uses.
   Suggested text:

   > **runFullTrust**: Snakecharmer is a Win32 tray app. It configures USB mice
   > through HID feature reports, shows a notification-area icon, and, only when the
   > user sets a thumb-button remap, uses a low-level mouse hook to handle those
   > buttons.
   >
   > **unvirtualizedResources**: the app's config file
   > (`%LOCALAPPDATA%\Snakecharmer\config.toml`) is documented for hand editing and
   > is shared with the app's command-line tool. The manifest excludes only that one
   > folder from file-system write virtualization so all of them read the same file.

7. **Store listing**: description, at least one screenshot (the tray menu and the
   settings window), and the logo. Keep listing copy factual about Razer: Snakecharmer
   is not affiliated with Razer, and the listing should say so.
8. **Age ratings**: complete the questionnaire (no user interaction, no data sharing).
9. Submit. Certification usually takes a few days.

## Releasing an update

1. Bump the version in `Cargo.toml` as usual. The package version is that plus `.0`;
   the Store reserves the fourth part.
2. Tag the release. The release workflow builds the `.msix` and attaches it to the
   workflow run as an artifact.
3. In Partner Center, start a new submission, replace the package, and submit.
