//! The MSIX package's `windows.startupTask` (TaskId `SnakecharmerStartup` in
//! `packaging/msix/AppxManifest.xml`), read and switched through the WinRT
//! `Windows.ApplicationModel.StartupTask` API. Only a packaged process can use
//! it; an unpackaged one gets [`TaskState::Unknown`] and uses the Run value
//! (see `autostart`).
//!
//! Windows lets the app turn its own task on and off, with one exception: once
//! the user turns it off in Settings or Task Manager, only the user can turn it
//! back on there ([`TaskState::OffInSettings`]).

use std::cell::Cell;

use windows::core::HSTRING;
use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

/// Must match `TaskId` in the manifest's `desktop:StartupTask`.
const TASK_ID: &str = "SnakecharmerStartup";

/// Where the user goes when the task is [`TaskState::OffInSettings`].
pub const SETTINGS_URI: &str = "ms-settings:startupapps";

/// The startup task's state, as Windows reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    On,
    Off,
    /// The user turned it off in Settings or Task Manager. The app cannot turn
    /// it back on; the user must, in Settings.
    OffInSettings,
    /// Group policy decides; neither the app nor the user can change it.
    SetByPolicy {
        on: bool,
    },
    /// The task could not be read (unpackaged, or a WinRT failure).
    Unknown,
}

impl TaskState {
    fn from_winrt(state: StartupTaskState) -> Self {
        match state {
            StartupTaskState::Enabled => Self::On,
            StartupTaskState::Disabled => Self::Off,
            StartupTaskState::DisabledByUser => Self::OffInSettings,
            StartupTaskState::DisabledByPolicy => Self::SetByPolicy { on: false },
            StartupTaskState::EnabledByPolicy => Self::SetByPolicy { on: true },
            _ => Self::Unknown,
        }
    }
}

/// Read the task's state now.
pub fn state() -> TaskState {
    task()
        .and_then(|task| task.State())
        .map(TaskState::from_winrt)
        .unwrap_or(TaskState::Unknown)
}

/// Turn the task on. Returns the state Windows reports afterwards, which stays
/// [`TaskState::OffInSettings`] if the user had turned it off there.
pub fn enable() -> TaskState {
    task()
        .and_then(|task| task.RequestEnableAsync()?.join())
        .map(TaskState::from_winrt)
        .unwrap_or(TaskState::Unknown)
}

/// Turn the task off, then read back its state.
pub fn disable() -> TaskState {
    match task().and_then(|task| task.Disable()) {
        Ok(()) => state(),
        Err(_) => TaskState::Unknown,
    }
}

fn task() -> windows::core::Result<StartupTask> {
    ensure_apartment();
    StartupTask::GetAsync(&HSTRING::from(TASK_ID))?.join()
}

thread_local! {
    static APARTMENT: Cell<bool> = const { Cell::new(false) };
}

/// WinRT calls need an initialized apartment on the calling thread. Join the
/// multithreaded one once per thread and keep it for the thread's lifetime.
/// A thread already in a single-threaded apartment returns `RPC_E_CHANGED_MODE`,
/// which is fine: it already has one.
fn ensure_apartment() {
    APARTMENT.with(|done| {
        if !done.get() {
            // SAFETY: RoInitialize has no pointer arguments; a failure only
            // means the thread already has an apartment.
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            done.set(true);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winrt_states_map() {
        assert_eq!(
            TaskState::from_winrt(StartupTaskState::Enabled),
            TaskState::On
        );
        assert_eq!(
            TaskState::from_winrt(StartupTaskState::Disabled),
            TaskState::Off
        );
        assert_eq!(
            TaskState::from_winrt(StartupTaskState::DisabledByUser),
            TaskState::OffInSettings
        );
        assert_eq!(
            TaskState::from_winrt(StartupTaskState::EnabledByPolicy),
            TaskState::SetByPolicy { on: true }
        );
        assert_eq!(
            TaskState::from_winrt(StartupTaskState(99)),
            TaskState::Unknown
        );
    }

    #[test]
    fn unpackaged_test_runner_reads_unknown() {
        assert_eq!(state(), TaskState::Unknown);
    }
}
