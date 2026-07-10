//! Smoke test for the WH_MOUSE_LL install/uninstall plumbing.
//!
//! Installs the hook on a message-pumping thread, confirms it reports installed,
//! pumps briefly, then uninstalls and confirms it's gone. It does NOT test
//! suppression/remap — that path deliberately ignores injected (LLMHF_INJECTED)
//! events, so it can only be validated with a physical thumb press.
//!
//!   cargo run -p platform --example hook_smoke

use platform::mouse_hook;

fn main() {
    // Configure a dummy remap so the hook state path is exercised.
    mouse_hook::set_thumb_actions(Some(vec![0x11, 0x43]), None); // Ctrl+C on Back

    let installed = mouse_hook::install();
    println!("install() -> {installed}");
    println!("is_installed() -> {}", mouse_hook::is_installed());
    assert!(installed && mouse_hook::is_installed(), "hook must install");

    // Pump the message loop briefly (a WH_MOUSE_LL hook needs its thread to pump);
    // a helper thread ends the loop after ~800 ms.
    let main_tid = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        unsafe {
            PostThreadMessageW(main_tid, WM_QUIT, 0, 0);
        }
    });

    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    mouse_hook::uninstall();
    println!("after uninstall(), is_installed() -> {}", mouse_hook::is_installed());
    assert!(!mouse_hook::is_installed(), "hook must uninstall");
    println!("Hook smoke test OK.");
}
