//! The share a program reported, shown outside the frame of the window.
//!
//! A window that says how far along it is says it where the desktop shows such
//! things — the button of the taskbar — so the share is there to be seen while
//! the window is covered by something else, which is exactly when somebody
//! wants it. Inside the window the status bar already shows it.
//!
//! Windows has the interface for it: `ITaskbarList3` of the shell, the same one
//! the file copier of the system draws its button with. The object is COM, it
//! belongs to the thread that created it, and the thread that draws this window
//! is the one asking — so it is created once per thread and kept.
//!
//! Wayland has nothing of the kind, and no protocol of it was ever proposed:
//! the compositor does not know what a taskbar is, and a window has no way to
//! say anything about itself beyond its title, its application id and its
//! icon. What the desktops of Linux answer to instead is a signal on the bus —
//! `com.canonical.Unity.LauncherEntry`, which KDE Plasma and the docks of GNOME
//! read — and it names the application by its desktop file rather than the
//! window, so a second window of the same program would paint over the share of
//! the first. That is why nothing is sent here.

use zyt_term::ProgressState;

/// Shows the share on the button of the window, where the platform has one.
///
/// Nothing said is nothing shown: a state of `None` takes the share away, which
/// is what the end of a program that reported one comes to.
#[cfg(target_os = "windows")]
pub fn show(frame: &eframe::Frame, state: Option<ProgressState>) {
    windows_taskbar::show(frame, state);
}

/// The platforms with no such button, where saying it is saying nothing.
#[cfg(not(target_os = "windows"))]
pub fn show(frame: &eframe::Frame, state: Option<ProgressState>) {
    let _ = (frame, state);
}

#[cfg(target_os = "windows")]
mod windows_taskbar {
    use super::ProgressState;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::OnceCell;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::{
        ITaskbarList3, TBPF_ERROR, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED,
        TBPFLAG, TaskbarList,
    };

    thread_local! {
        /// The shell object of this thread, asked for once.
        ///
        /// It is asked for once whether or not the answer was one, because a
        /// desktop without the interface — a session with no shell, an older
        /// Windows — would otherwise be asked again every time a program said
        /// how far it had got.
        static TASKBAR: OnceCell<Option<ITaskbarList3>> = const { OnceCell::new() };
    }

    /// Hands the state to the taskbar, and says nothing when it cannot.
    pub fn show(frame: &eframe::Frame, state: Option<ProgressState>) {
        let Some(window) = handle(frame) else {
            return;
        };

        TASKBAR.with(|taskbar| {
            let Some(taskbar) = taskbar.get_or_init(create) else {
                return;
            };
            let (flag, share) = wanted(state);
            // Safety: the object was created on this thread and is used on it,
            // and the handle is the window this frame is drawn in.
            unsafe {
                if let Err(error) = taskbar.SetProgressState(window, flag) {
                    log::debug!("the taskbar refused the state: {error}");
                }
                if let Some(share) = share
                    && let Err(error) = taskbar.SetProgressValue(window, u64::from(share), 100)
                {
                    log::debug!("the taskbar refused the share: {error}");
                }
            }
        });
    }

    /// What the button is asked to show, and the share to fill it with.
    fn wanted(state: Option<ProgressState>) -> (TBPFLAG, Option<u8>) {
        match state {
            None | Some(ProgressState::Removed) => (TBPF_NOPROGRESS, None),
            Some(ProgressState::Set(share)) => (TBPF_NORMAL, Some(share)),
            Some(ProgressState::Error(share)) => (TBPF_ERROR, Some(share)),
            Some(ProgressState::Paused(share)) => (TBPF_PAUSED, Some(share)),
            Some(ProgressState::Indeterminate) => (TBPF_INDETERMINATE, None),
        }
    }

    /// The window of this frame, as the shell wants it.
    fn handle(frame: &eframe::Frame) -> Option<HWND> {
        let handle = frame.window_handle().ok()?;
        let RawWindowHandle::Win32(window) = handle.as_raw() else {
            return None;
        };
        Some(HWND(window.hwnd.get() as *mut core::ffi::c_void))
    }

    /// The shell object of this thread, or nothing when the desktop has none.
    fn create() -> Option<ITaskbarList3> {
        // Safety: both are the ordinary way of reaching a COM object of the
        // shell, and this thread is the one that goes on using it.
        unsafe {
            let started = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if started.is_err() {
                log::debug!("COM was not started on the drawing thread: {started:?}");
            }
            let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_ALL)
                .inspect_err(|error| log::debug!("the desktop has no taskbar list: {error}"))
                .ok()?;
            taskbar
                .HrInit()
                .inspect_err(|error| log::debug!("the taskbar list did not start: {error}"))
                .ok()?;
            Some(taskbar)
        }
    }
}
