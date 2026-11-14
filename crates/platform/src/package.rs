//! Whether this process runs with MSIX package identity (installed from the
//! Microsoft Store or a sideloaded `.msix`) or as a bare exe.
//!
//! A packaged copy differs in two ways the rest of the app cares about:
//! login startup belongs to the package's `windows.startupTask` extension
//! (see `startup_task`) rather than the HKCU Run value, and HKCU registry writes land in the
//! package's private hive, where the Run value would have no effect.

use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// True when the current process has package identity.
pub fn is_packaged() -> bool {
    let mut len: u32 = 0;
    // SAFETY: a zero length with a null buffer is the documented size query;
    // the call only writes `len`.
    let status = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    classify(status)
}

/// A packaged process answers the size query with `ERROR_INSUFFICIENT_BUFFER`;
/// an unpackaged one with `APPMODEL_ERROR_NO_PACKAGE`. Anything else is
/// treated as unpackaged, which keeps the long-standing Run-value behavior.
fn classify(status: u32) -> bool {
    status == ERROR_INSUFFICIENT_BUFFER
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;

    #[test]
    fn size_query_statuses_classify() {
        assert!(classify(ERROR_INSUFFICIENT_BUFFER));
        assert!(!classify(APPMODEL_ERROR_NO_PACKAGE));
        assert!(!classify(0));
    }

    #[test]
    fn test_runner_is_unpackaged() {
        assert!(!is_packaged());
    }
}
