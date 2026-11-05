//! Default layout, described as data.

use crate::error::Result;
use crate::keymap::{Binding, Keymap};

/// One entry of the default layout.
#[derive(Debug, Clone, Copy)]
pub struct DefaultBinding {
    /// Context the binding belongs to.
    pub context: &'static str,
    /// Key sequence in text form.
    pub keys: &'static str,
    /// Command triggered by the sequence.
    pub command: &'static str,
    /// It repeats by its last key: [`Binding::repeat`].
    pub repeat: bool,
}

/// Context that is active regardless of focus.
pub const CONTEXT_GLOBAL: &str = "global";
/// Context active while the terminal has focus.
pub const CONTEXT_TERMINAL: &str = "terminal";
/// Context active over the terminal while it shows tmux and its keys are on.
pub const CONTEXT_TMUX: &str = "tmux";
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

const fn bind(context: &'static str, keys: &'static str, command: &'static str) -> DefaultBinding {
    DefaultBinding {
        context,
        keys,
        command,
        repeat: false,
    }
}

const fn again(context: &'static str, keys: &'static str, command: &'static str) -> DefaultBinding {
    DefaultBinding {
        repeat: true,
        ..bind(context, keys, command)
    }
}

use {
    CONTEXT_GLOBAL as GLOBAL, CONTEXT_SEARCH as SEARCH, CONTEXT_SETTINGS as SETTINGS,
    CONTEXT_STATUS_BAR as STATUS_BAR, CONTEXT_TERMINAL as TERMINAL, CONTEXT_TMUX as TMUX,
    CONTEXT_TRANSFER as TRANSFER,
};

/// Layout shipped with the application.
///
/// Leaving the application is not in it. A window holding an open port and a
/// running console is a window nobody wants closed by a key struck beside the
/// one that was meant; the command stands in the palette, where it is asked for
/// by name. Somebody who wants a key for it binds one.
///
/// The keys of tmux are those of tmux after its prefix, `ctrl+b`; a key with
/// `shift` is bound by its character on a US keyboard, `%` or `E`.
pub const DEFAULT_BINDINGS: &[DefaultBinding] = &[
    bind(TRANSFER, "ctrl+c", "transfer.cancel"),
    bind(TRANSFER, "ctrl+escape", "transfer.force"),
    bind(GLOBAL, "ctrl+shift+p", "palette.open"),
    bind(GLOBAL, "ctrl+shift+s", "settings.open"),
    bind(GLOBAL, "ctrl+shift+b", "view.toggle_status_bar"),
    bind(GLOBAL, "ctrl+shift+n", "app.new_window"),
    bind(TERMINAL, "ctrl+shift+tab", "focus.status_bar"),
    bind(STATUS_BAR, "ctrl+shift+tab", "focus.terminal"),
    bind(STATUS_BAR, "escape", "focus.terminal"),
    bind(TERMINAL, "ctrl+shift+c", "terminal.copy"),
    bind(TERMINAL, "ctrl+shift+v", "terminal.paste"),
    bind(TERMINAL, "ctrl+shift+o", "port.choose"),
    bind(TERMINAL, "ctrl+shift+k", "terminal.clear"),
    bind(TERMINAL, "shift+pageup", "terminal.scroll_page_up"),
    bind(TERMINAL, "shift+pagedown", "terminal.scroll_page_down"),
    bind(SETTINGS, "escape", "settings.close"),
    bind(SETTINGS, "ctrl+shift+o", "port.choose"),
    bind(TERMINAL, "ctrl+shift+f", "search.open"),
    bind(SEARCH, "ctrl+shift+f", "search.previous"),
    bind(SEARCH, "ctrl+f", "search.next"),
    bind(SEARCH, "escape", "search.close"),
    bind(TERMINAL, "ctrl+shift+r", "history.open"),
    bind(TERMINAL, "ctrl+shift+up", "terminal.select_up"),
    bind(TERMINAL, "ctrl+shift+down", "terminal.select_down"),
    bind(TERMINAL, "ctrl+shift+left", "terminal.select_left"),
    bind(TERMINAL, "ctrl+shift+right", "terminal.select_right"),
    bind(TERMINAL, "ctrl+shift+enter", "terminal.block_input"),
    bind(TERMINAL, "ctrl+shift+home", "terminal.select_to_line_start"),
    bind(TERMINAL, "ctrl+shift+end", "terminal.select_to_line_end"),
    bind(TMUX, "ctrl+b c", "tmux.window.new"),
    bind(TMUX, "ctrl+b &", "tmux.window.close"),
    bind(TMUX, "ctrl+b ,", "tmux.window.rename"),
    bind(TMUX, "ctrl+b n", "tmux.window.next"),
    bind(TMUX, "ctrl+b p", "tmux.window.previous"),
    bind(TMUX, "ctrl+b l", "tmux.window.last"),
    bind(TMUX, "ctrl+b 0", "tmux.window.0"),
    bind(TMUX, "ctrl+b 1", "tmux.window.1"),
    bind(TMUX, "ctrl+b 2", "tmux.window.2"),
    bind(TMUX, "ctrl+b 3", "tmux.window.3"),
    bind(TMUX, "ctrl+b 4", "tmux.window.4"),
    bind(TMUX, "ctrl+b 5", "tmux.window.5"),
    bind(TMUX, "ctrl+b 6", "tmux.window.6"),
    bind(TMUX, "ctrl+b 7", "tmux.window.7"),
    bind(TMUX, "ctrl+b 8", "tmux.window.8"),
    bind(TMUX, "ctrl+b 9", "tmux.window.9"),
    bind(TMUX, "ctrl+b space", "tmux.window.layout_next"),
    bind(TMUX, "ctrl+b alt+1", "tmux.window.layout_1"),
    bind(TMUX, "ctrl+b alt+2", "tmux.window.layout_2"),
    bind(TMUX, "ctrl+b alt+3", "tmux.window.layout_3"),
    bind(TMUX, "ctrl+b alt+4", "tmux.window.layout_4"),
    bind(TMUX, "ctrl+b alt+5", "tmux.window.layout_5"),
    bind(TMUX, "ctrl+b %", "tmux.pane.split_right"),
    bind(TMUX, "ctrl+b \"", "tmux.pane.split_below"),
    bind(TMUX, "ctrl+b x", "tmux.pane.close"),
    bind(TMUX, "ctrl+b !", "tmux.pane.break"),
    bind(TMUX, "ctrl+b z", "tmux.pane.zoom"),
    bind(TMUX, "ctrl+b {", "tmux.pane.swap_previous"),
    bind(TMUX, "ctrl+b }", "tmux.pane.swap_next"),
    again(TMUX, "ctrl+b up", "tmux.pane.focus_up"),
    again(TMUX, "ctrl+b down", "tmux.pane.focus_down"),
    again(TMUX, "ctrl+b left", "tmux.pane.focus_left"),
    again(TMUX, "ctrl+b right", "tmux.pane.focus_right"),
    bind(TMUX, "ctrl+b o", "tmux.pane.focus_next"),
    bind(TMUX, "ctrl+b ;", "tmux.pane.focus_last"),
    bind(TMUX, "ctrl+b q", "tmux.pane.numbers"),
    again(TMUX, "ctrl+b ctrl+up", "tmux.pane.resize_up"),
    again(TMUX, "ctrl+b ctrl+down", "tmux.pane.resize_down"),
    again(TMUX, "ctrl+b ctrl+left", "tmux.pane.resize_left"),
    again(TMUX, "ctrl+b ctrl+right", "tmux.pane.resize_right"),
    again(TMUX, "ctrl+b alt+up", "tmux.pane.resize_up_5"),
    again(TMUX, "ctrl+b alt+down", "tmux.pane.resize_down_5"),
    again(TMUX, "ctrl+b alt+left", "tmux.pane.resize_left_5"),
    again(TMUX, "ctrl+b alt+right", "tmux.pane.resize_right_5"),
    bind(TMUX, "ctrl+b $", "tmux.session.rename"),
    bind(TMUX, "ctrl+b (", "tmux.session.previous"),
    bind(TMUX, "ctrl+b )", "tmux.session.next"),
    bind(TMUX, "ctrl+b d", "tmux.session.detach"),
    bind(TMUX, "ctrl+b ctrl+b", "tmux.prefix"),
];

/// Key map built from [`DEFAULT_BINDINGS`].
pub fn default_keymap() -> Result<Keymap> {
    let mut keymap = Keymap::new();
    for entry in DEFAULT_BINDINGS {
        let binding = Binding::parse(entry.keys, entry.command, entry.repeat)?;
        keymap.bind(entry.context, binding)?;
    }
    Ok(keymap)
}
