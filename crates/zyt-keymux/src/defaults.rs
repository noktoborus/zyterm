//! Default layout, described as data.

use crate::error::Result;
use crate::keymap::Keymap;

/// One entry of the default layout.
#[derive(Debug, Clone, Copy)]
pub struct DefaultBinding {
    /// Context the binding belongs to.
    pub context: &'static str,
    /// Key sequence in text form.
    pub keys: &'static str,
    /// Command triggered by the sequence.
    pub command: &'static str,
}

/// Context that is active regardless of focus.
pub const CONTEXT_GLOBAL: &str = "global";
/// Context active while the terminal has focus.
pub const CONTEXT_TERMINAL: &str = "terminal";
/// Context active while the settings window has focus.
pub const CONTEXT_SETTINGS: &str = "settings";
/// Context active while a menu stands over the interface — the command palette
/// and every other menu of the caller.
///
/// A menu reads the keys it is walked by, so the bindings of whatever it was
/// opened from are out of reach until it is closed: one key must not mean a step
/// through the menu and something else besides.
pub const CONTEXT_PALETTE: &str = "palette";
/// Context active while the status bar has the keyboard.
pub const CONTEXT_STATUS_BAR: &str = "statusbar";
/// Context active while the search bar has the keyboard.
pub const CONTEXT_SEARCH: &str = "search";
/// Context active while a transfer is running.
///
/// A transfer takes the keyboard away from the device: the program on the
/// other end is in the middle of a protocol, and a byte typed into that would
/// be read as part of it. The one thing a key may still do is stop it.
pub const CONTEXT_TRANSFER: &str = "transfer";

/// Layout shipped with the application.
///
/// Leaving the application is not in it. A window holding an open port and a
/// running console is a window nobody wants closed by a key struck beside the
/// one that was meant; the command stands in the palette, where it is asked for
/// by name. Somebody who wants a key for it binds one.
pub const DEFAULT_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        context: CONTEXT_TRANSFER,
        keys: "ctrl+c",
        command: "transfer.cancel",
    },
    DefaultBinding {
        context: CONTEXT_GLOBAL,
        keys: "ctrl+shift+p",
        command: "palette.open",
    },
    DefaultBinding {
        context: CONTEXT_GLOBAL,
        keys: "ctrl+shift+s",
        command: "settings.open",
    },
    DefaultBinding {
        context: CONTEXT_GLOBAL,
        keys: "ctrl+shift+b",
        command: "view.toggle_status_bar",
    },
    DefaultBinding {
        context: CONTEXT_GLOBAL,
        keys: "ctrl+shift+n",
        command: "app.new_window",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+tab",
        command: "focus.status_bar",
    },
    DefaultBinding {
        context: CONTEXT_STATUS_BAR,
        keys: "ctrl+shift+tab",
        command: "focus.terminal",
    },
    DefaultBinding {
        context: CONTEXT_STATUS_BAR,
        keys: "escape",
        command: "focus.terminal",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+c",
        command: "terminal.copy",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+v",
        command: "terminal.paste",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+o",
        command: "port.choose",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+k",
        command: "terminal.clear",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "shift+pageup",
        command: "terminal.scroll_page_up",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "shift+pagedown",
        command: "terminal.scroll_page_down",
    },
    DefaultBinding {
        context: CONTEXT_SETTINGS,
        keys: "escape",
        command: "settings.close",
    },
    DefaultBinding {
        context: CONTEXT_SETTINGS,
        keys: "ctrl+shift+o",
        command: "port.choose",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+f",
        command: "search.open",
    },
    DefaultBinding {
        context: CONTEXT_SEARCH,
        keys: "ctrl+shift+f",
        command: "search.previous",
    },
    DefaultBinding {
        context: CONTEXT_SEARCH,
        keys: "ctrl+f",
        command: "search.next",
    },
    DefaultBinding {
        context: CONTEXT_SEARCH,
        keys: "escape",
        command: "search.close",
    },
    DefaultBinding {
        context: CONTEXT_TERMINAL,
        keys: "ctrl+shift+r",
        command: "history.open",
    },
];

/// Key map built from [`DEFAULT_BINDINGS`].
pub fn default_keymap() -> Result<Keymap> {
    let mut keymap = Keymap::new();
    for binding in DEFAULT_BINDINGS {
        keymap.bind(binding.context, binding.keys, binding.command)?;
    }
    Ok(keymap)
}
