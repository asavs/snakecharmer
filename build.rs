//! Embed the comctl32 v6 manifest (themed visual-styles controls for the
//! settings window) into the `snakecharmer` and `charmctl` binaries at link
//! time. The manifest lives with the Win32 layer that needs it —
//! `crates/platform/comctl32-v6.manifest` — and `crates/platform/build.rs`
//! embeds the same file into that crate's examples. MSVC linker flags only —
//! no extra build dependency and nothing to ship beside the exe.

fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/platform/comctl32-v6.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
