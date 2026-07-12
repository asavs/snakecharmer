//! Embed the comctl32 v6 manifest into this crate's *examples* (the settings
//! smoke test), so they get themed visual-styles controls like the real bins.
//! The workspace root `build.rs` does the same for the `snakecharmer` /
//! `charmctl` binaries. MSVC linker flags only — no extra build dependency.

fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("comctl32-v6.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-examples=/MANIFESTINPUT:{}", manifest.display());
    }
}
