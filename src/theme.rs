//! Interface colors, following the desktop setting when asked to.

use crate::config::ThemeMode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use zyt_term_egui::TerminalTheme;

/// How often the desktop setting is read when no change notification is
/// available. With a working notification the setting is never polled.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(60);

/// Tracks the effective color mode.
///
/// The desktop setting is delivered by the platform as a change notification,
/// which on Linux is one D-Bus signal of the desktop portal. Polling it would
/// mean a D-Bus round trip per tick, so it only happens when the platform
/// cannot notify.
pub struct ThemeWatcher {
    mode: ThemeMode,
    dark: bool,
    changed: Arc<AtomicBool>,
    watching: bool,
    context: egui::Context,
    last_check: Instant,
}

impl ThemeWatcher {
    /// Watcher for the configured mode.
    ///
    /// The window is handed over because a notification that nobody is awake
    /// for is a notification nobody reads: the desktop says the colors changed
    /// whenever it likes, and a window with nothing to draw draws nothing until
    /// something asks it to.
    pub fn new(mode: ThemeMode, context: egui::Context) -> Self {
        let mut watcher = Self {
            mode,
            dark: true,
            changed: Arc::new(AtomicBool::new(false)),
            watching: false,
            context,
            last_check: Instant::now(),
        };
        watcher.dark = watcher.detect();
        watcher.follow_system();
        watcher
    }

    /// Changes the configured mode.
    pub fn set_mode(&mut self, mode: ThemeMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.dark = self.detect();
        self.follow_system();
    }

    /// Applies pending changes of the desktop setting. Returns true on change.
    pub fn poll(&mut self) -> bool {
        if self.mode != ThemeMode::System {
            return false;
        }

        if self.watching {
            if !self.changed.swap(false, Ordering::Relaxed) {
                return false;
            }
            let dark = self.detect();
            let changed = dark != self.dark;
            self.dark = dark;
            return changed;
        }

        if self.last_check.elapsed() < FALLBACK_INTERVAL {
            return false;
        }
        self.last_check = Instant::now();
        let dark = self.detect();
        let changed = dark != self.dark;
        self.dark = dark;
        changed
    }

    /// True while the dark colors are in use.
    pub fn is_dark(&self) -> bool {
        self.dark
    }

    /// egui visuals for the current mode.
    pub fn visuals(&self) -> egui::Visuals {
        if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        }
    }

    /// Terminal palette for the current mode.
    pub fn terminal_theme(&self) -> TerminalTheme {
        if self.dark {
            TerminalTheme::dark()
        } else {
            TerminalTheme::light()
        }
    }

    /// Starts the change notification of the platform, once.
    ///
    /// The notification arrives on a channel, and a channel nobody is waiting
    /// on says nothing: it is waited on in a thread of its own, which marks the
    /// change and wakes the window for it. What the desktop said is not read
    /// there — [`ThemeWatcher::poll`] asks the platform again on the drawing
    /// thread — so the thread decides nothing and only says that there is
    /// something to decide.
    ///
    /// It is started once and left running. Switching away from following the
    /// desktop stops the colors from changing — `poll` answers nothing for any
    /// other mode — and switching back needs no second thread; one that was
    /// stopped and started with every turn of that setting would be a thread
    /// per turn of it.
    fn follow_system(&mut self) {
        if self.mode != ThemeMode::System || self.watching {
            return;
        }

        let watcher = match dark_light::subscribe() {
            Ok(watcher) => watcher,
            Err(error) => {
                log::debug!("no theme notification, falling back to polling: {error}");
                self.last_check = Instant::now();
                return;
            }
        };

        let changed = self.changed.clone();
        let context = self.context.clone();
        let started = std::thread::Builder::new()
            .name("zyterm-theme".to_string())
            .spawn(move || {
                while watcher.recv().is_ok() {
                    changed.store(true, Ordering::Relaxed);
                    context.request_repaint();
                }
            });
        match started {
            Ok(_) => self.watching = true,
            Err(error) => {
                log::debug!("no thread for the theme notification: {error}");
                self.last_check = Instant::now();
            }
        }
    }

    fn detect(&self) -> bool {
        match self.mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => is_dark(dark_light::detect()),
        }
    }
}

/// Dark is the default when the platform reports nothing usable.
fn is_dark(mode: Result<dark_light::Mode, dark_light::Error>) -> bool {
    !matches!(mode, Ok(dark_light::Mode::Light))
}
