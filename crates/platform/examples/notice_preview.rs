//! Renders the daemon's "found a mouse I can't drive yet" notice, so its
//! wording and wrapping can be checked without owning an unsupported device.
//!
//!   cargo run -p platform --example notice_preview
//!
//! The daemon builds the same message in `src/daemon.rs` from whatever
//! `razer_hid::detected_mice` reports; the device named here is a stand-in.
fn main() {
    let found = "Razer DeathAdder Chroma (1532:0043)";
    let open = platform::alert_yes_no(
        "Snakecharmer",
        &format!(
            "Found {found}, which isn't supported yet.\n\n\
             Its protocol is very likely already documented, so adding it \
             is a small job. You may only need to report a couple of \
             numbers from the device.\n\n\
             Open the page that lists what's already known about it?"
        ),
    );
    println!("open = {open}");
}
