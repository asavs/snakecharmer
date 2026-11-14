#!/usr/bin/env python3
"""Build the MSIX package for the Microsoft Store.

    python tools/build-msix.py              # release build -> target/msix/Snakecharmer_<ver>_x64.msix
    python tools/build-msix.py --skip-build # package the exes already in target/release
    python tools/build-msix.py --register   # also install the staged layout for a local test

Steps: `cargo build --release` for snakecharmer and charmctl, the static-CRT check,
stage a layout (exes, Assets, a filled-in AppxManifest.xml), index the images into
resources.pri with makepri, then pack with makeappx. Both tools ship with the
Windows 10/11 SDK; the newest installed SDK is used.

The package is unsigned on purpose: the Store signs what it publishes. `--register`
installs the unsigned layout directly, which needs Developer Mode
(Settings > System > For developers). Quit any running Snakecharmer first, or the
release build cannot replace the locked exe.

Identity comes from packaging/msix/identity.toml, version from Cargo.toml.
"""
import argparse
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGING = ROOT / "packaging" / "msix"
OUT = ROOT / "target" / "msix"
LAYOUT = OUT / "layout"
EXES = ["snakecharmer.exe", "charmctl.exe"]
SDK_BIN = Path(r"C:\Program Files (x86)\Windows Kits\10\bin")


def fail(msg: str) -> None:
    sys.exit(f"build-msix: {msg}")


def run(cmd: list, **kw) -> None:
    print("+", " ".join(str(c) for c in cmd), flush=True)
    result = subprocess.run([str(c) for c in cmd], **kw)
    if result.returncode != 0:
        fail(f"{Path(str(cmd[0])).name} exited with {result.returncode}")


def sdk_tool(name: str) -> Path:
    """The newest installed SDK's x64 copy of `name`."""
    def version_key(path: Path):
        return tuple(int(p) for p in path.parent.parent.name.split("."))

    found = sorted(
        (p for p in SDK_BIN.glob(f"10.*/x64/{name}") if p.is_file()),
        key=version_key,
    )
    if not found:
        fail(f"{name} not found under {SDK_BIN}; install the Windows SDK")
    return found[-1]


def package_version() -> str:
    """Cargo's x.y.z as the Store's x.y.z.0 (the Store reserves the fourth part)."""
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = cargo["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        fail(f"Cargo version {version!r} is not plain x.y.z")
    return f"{version}.0"


def identity() -> dict:
    data = tomllib.loads((PACKAGING / "identity.toml").read_text(encoding="utf-8"))
    for key in ("name", "publisher", "publisher_display_name"):
        if not data.get(key):
            fail(f"identity.toml is missing {key}")
    return data


def xml_escape(text: str) -> str:
    return (
        text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")
    )


def write_manifest(version: str, ident: dict) -> None:
    template = (PACKAGING / "AppxManifest.xml").read_text(encoding="utf-8")
    fields = {
        "IDENTITY_NAME": ident["name"],
        "PUBLISHER": ident["publisher"],
        "PUBLISHER_DISPLAY_NAME": ident["publisher_display_name"],
        "VERSION": version,
    }
    for key, value in fields.items():
        template = template.replace("{{" + key + "}}", xml_escape(value))
    leftover = re.findall(r"\{\{\w+\}\}", template)
    if leftover:
        fail(f"unfilled manifest fields: {leftover}")
    (LAYOUT / "AppxManifest.xml").write_text(template, encoding="utf-8")


def stage(version: str, ident: dict) -> None:
    if LAYOUT.exists():
        shutil.rmtree(LAYOUT)
    LAYOUT.mkdir(parents=True)
    for exe in EXES:
        src = ROOT / "target" / "release" / exe
        if not src.exists():
            fail(f"{src} is missing; run without --skip-build")
        shutil.copy2(src, LAYOUT / exe)
    shutil.copytree(PACKAGING / "Assets", LAYOUT / "Assets")
    shutil.copy2(ROOT / "LICENSE", LAYOUT / "LICENSE.txt")
    write_manifest(version, ident)


def make_pri() -> None:
    makepri = sdk_tool("makepri.exe")
    config = OUT / "priconfig.xml"
    run([makepri, "createconfig", "/cf", config, "/dq", "en-US", "/pv", "10.0.0", "/o"],
        stdout=subprocess.DEVNULL)
    run([makepri, "new", "/pr", LAYOUT, "/cf", config,
         "/mn", LAYOUT / "AppxManifest.xml", "/of", LAYOUT / "resources.pri", "/o"],
        stdout=subprocess.DEVNULL)


def pack(version: str) -> Path:
    msix = OUT / f"Snakecharmer_{version}_x64.msix"
    run([sdk_tool("makeappx.exe"), "pack", "/d", LAYOUT, "/p", msix, "/o"],
        stdout=subprocess.DEVNULL)
    return msix


def register() -> None:
    manifest = LAYOUT / "AppxManifest.xml"
    run(["powershell", "-NoProfile", "-Command",
         f"Add-AppxPackage -Register '{manifest}' -ForceApplicationShutdown"])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--skip-build", action="store_true", help="package target/release as is")
    parser.add_argument("--register", action="store_true",
                        help="install the staged layout (Developer Mode required)")
    args = parser.parse_args()

    if not args.skip_build:
        run(["cargo", "build", "--release", "--bin", "snakecharmer", "--bin", "charmctl"], cwd=ROOT)
    run([sys.executable, ROOT / "tools" / "check-static-crt.py",
         *(ROOT / "target" / "release" / exe for exe in EXES)])

    version = package_version()
    stage(version, identity())
    make_pri()
    msix = pack(version)
    print(f"built {msix.relative_to(ROOT)}")
    if args.register:
        register()
        print("registered the layout; find Snakecharmer in the Start menu")


if __name__ == "__main__":
    main()
