//! The selection the middle mouse button pastes.
//!
//! On Linux a selection made with the mouse goes into the primary selection,
//! which the middle button pastes; it is separate from the clipboard the copy
//! command uses. Windows has no such selection, so there the middle button
//! falls back to the selection of this terminal and to the clipboard.

/// Holds the selection of this window, when the platform has one.
#[derive(Default)]
pub struct Primary {
    #[cfg(target_os = "linux")]
    clipboard: Option<arboard::Clipboard>,
}

impl std::fmt::Debug for Primary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Primary").finish()
    }
}

impl Primary {
    /// True when the platform has a selection of its own.
    pub fn exists() -> bool {
        cfg!(target_os = "linux")
    }

    /// Text of the selection, when the platform has one and it holds text.
    #[cfg(target_os = "linux")]
    pub fn text(&mut self) -> Option<String> {
        use arboard::{GetExtLinux, LinuxClipboardKind};

        let clipboard = self.clipboard()?;
        match clipboard
            .get()
            .clipboard(LinuxClipboardKind::Primary)
            .text()
        {
            Ok(text) if !text.is_empty() => Some(text),
            Ok(_) => None,
            Err(error) => {
                log::debug!("primary selection: {error}");
                None
            }
        }
    }

    /// Text of the selection, when the platform has one and it holds text.
    #[cfg(not(target_os = "linux"))]
    pub fn text(&mut self) -> Option<String> {
        None
    }

    /// Puts the text of a selection into the selection of the platform.
    #[cfg(target_os = "linux")]
    pub fn set(&mut self, text: &str) {
        use arboard::{LinuxClipboardKind, SetExtLinux};

        let Some(clipboard) = self.clipboard() else {
            return;
        };
        if let Err(error) = clipboard
            .set()
            .clipboard(LinuxClipboardKind::Primary)
            .text(text.to_string())
        {
            log::debug!("primary selection: {error}");
        }
    }

    /// Puts the text of a selection into the selection of the platform.
    #[cfg(not(target_os = "linux"))]
    pub fn set(&mut self, _text: &str) {}

    /// The handle, opened once and kept, because the selection lives as long as
    /// the process that owns it.
    #[cfg(target_os = "linux")]
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    log::debug!("no selection from the platform: {error}");
                    return None;
                }
            }
        }
        self.clipboard.as_mut()
    }
}
