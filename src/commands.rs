//! Commands of the application and the registry used by the palette.

use rust_i18n::t;
use zyt_keymux::{
    CONTEXT_GLOBAL, CONTEXT_SEARCH, CONTEXT_SETTINGS, CONTEXT_TERMINAL, Command, CommandId,
    CommandRegistry, Context,
};

/// One action the user can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    /// Open the command palette.
    PaletteOpen,
    /// Show the settings, or leave them when they are already shown.
    SettingsOpen,
    /// Close the settings window.
    SettingsClose,
    /// Show or hide the status bar.
    ToggleStatusBar,
    /// Close and open the port again.
    PortReopen,
    /// End the connection.
    PortDisconnect,
    /// Show the sources this window can open.
    PortChoose,
    /// Open the port that was connected before the last disconnect.
    PortOpenPrevious,
    /// Hold the transmission line in the break condition, or let it go.
    PortToggleBreak,
    /// Stop reading the port, or begin again.
    PortToggleHold,
    /// Throw away what is still waiting to go out to the device.
    PortDiscardOutput,
    /// Leave the plate of the signals standing, or take it down.
    ToggleSignals,
    /// Leave the application.
    Quit,
    /// Start another window.
    NewWindow,
    /// Give the keyboard to the status bar.
    FocusStatusBar,
    /// Give the keyboard back to the terminal.
    FocusTerminal,
    /// Copy the selection.
    Copy,
    /// Paste the clipboard.
    Paste,
    /// Clear the screen and the scrollback.
    Clear,
    /// Scroll one page up.
    ScrollPageUp,
    /// Scroll one page down.
    ScrollPageDown,
    /// Send a file with the active profile.
    SendFile,
    /// Receive a file with the active profile.
    ReceiveFile,
    /// Stop a running transfer.
    CancelTransfer,
    /// Show the search bar in place of the status bar.
    SearchOpen,
    /// Step to the next match towards the newest line.
    SearchNext,
    /// Step to the next match towards the beginning of the scrollback.
    SearchPrevious,
    /// Leave the search and take its marks down.
    SearchClose,
    /// Open the commands the shell of this source marked.
    HistoryOpen,
    /// Take the mouse from the program that asked for it, or hand it back.
    ToggleMouseReports,
}

impl AppCommand {
    /// Identifier used in the key map.
    pub fn id(self) -> &'static str {
        match self {
            Self::PaletteOpen => "palette.open",
            Self::SettingsOpen => "settings.open",
            Self::SettingsClose => "settings.close",
            Self::ToggleStatusBar => "view.toggle_status_bar",
            Self::PortReopen => "port.reopen",
            Self::PortDisconnect => "port.disconnect",
            Self::PortChoose => "port.choose",
            Self::PortOpenPrevious => "port.open_previous",
            Self::PortToggleBreak => "port.toggle_break",
            Self::PortToggleHold => "port.toggle_hold",
            Self::PortDiscardOutput => "port.discard_output",
            Self::ToggleSignals => "view.toggle_signals",
            Self::Quit => "app.quit",
            Self::NewWindow => "app.new_window",
            Self::FocusStatusBar => "focus.status_bar",
            Self::FocusTerminal => "focus.terminal",
            Self::Copy => "terminal.copy",
            Self::Paste => "terminal.paste",
            Self::Clear => "terminal.clear",
            Self::ScrollPageUp => "terminal.scroll_page_up",
            Self::ScrollPageDown => "terminal.scroll_page_down",
            Self::SendFile => "transfer.send_file",
            Self::ReceiveFile => "transfer.receive_file",
            Self::CancelTransfer => "transfer.cancel",
            Self::SearchOpen => "search.open",
            Self::SearchNext => "search.next",
            Self::SearchPrevious => "search.previous",
            Self::SearchClose => "search.close",
            Self::HistoryOpen => "history.open",
            Self::ToggleMouseReports => "terminal.mouse_reports",
        }
    }

    /// Context the command belongs to.
    pub fn context(self) -> &'static str {
        match self {
            Self::SettingsClose => CONTEXT_SETTINGS,
            Self::PortOpenPrevious => CONTEXT_GLOBAL,
            Self::PortDisconnect
            | Self::PortChoose
            | Self::PortToggleBreak
            | Self::PortToggleHold
            | Self::PortDiscardOutput
            | Self::ToggleSignals => CONTEXT_TERMINAL,
            Self::FocusTerminal => zyt_keymux::CONTEXT_STATUS_BAR,
            Self::FocusStatusBar => CONTEXT_TERMINAL,
            Self::Copy
            | Self::Paste
            | Self::Clear
            | Self::ScrollPageUp
            | Self::ScrollPageDown
            | Self::SendFile
            | Self::ReceiveFile
            | Self::CancelTransfer
            | Self::SearchOpen
            | Self::HistoryOpen
            | Self::ToggleMouseReports => CONTEXT_TERMINAL,
            Self::SearchNext | Self::SearchPrevious | Self::SearchClose => CONTEXT_SEARCH,
            _ => CONTEXT_GLOBAL,
        }
    }

    /// Translation key of the title.
    pub fn title_key(self) -> String {
        format!("command.{}", self.id())
    }

    /// Command with the given identifier.
    pub fn from_id(id: &CommandId) -> Option<Self> {
        ALL.iter()
            .copied()
            .find(|command| command.id() == id.as_str())
    }
}

/// Every command of the application.
pub const ALL: &[AppCommand] = &[
    AppCommand::PaletteOpen,
    AppCommand::SettingsOpen,
    AppCommand::SettingsClose,
    AppCommand::ToggleStatusBar,
    AppCommand::PortReopen,
    AppCommand::PortDisconnect,
    AppCommand::PortChoose,
    AppCommand::PortOpenPrevious,
    AppCommand::PortToggleBreak,
    AppCommand::PortToggleHold,
    AppCommand::PortDiscardOutput,
    AppCommand::ToggleSignals,
    AppCommand::Quit,
    AppCommand::NewWindow,
    AppCommand::FocusStatusBar,
    AppCommand::FocusTerminal,
    AppCommand::Copy,
    AppCommand::Paste,
    AppCommand::Clear,
    AppCommand::ScrollPageUp,
    AppCommand::ScrollPageDown,
    AppCommand::SendFile,
    AppCommand::ReceiveFile,
    AppCommand::CancelTransfer,
    AppCommand::SearchOpen,
    AppCommand::SearchNext,
    AppCommand::SearchPrevious,
    AppCommand::SearchClose,
    AppCommand::HistoryOpen,
    AppCommand::ToggleMouseReports,
];

/// Registry filled with the translated titles of the current locale.
pub fn build_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    for command in ALL {
        registry.add(Command {
            id: CommandId::new(command.id()),
            context: Context::new(command.context()),
            title: {
                let key = command.title_key();
                t!(&key).to_string()
            },
        });
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command is named in the palette, so every one of them needs a
    /// title in both languages. The list of commands is here, so the check is
    /// here as well: a command added without its text would otherwise show the
    /// key of its title to the user.
    #[test]
    fn every_command_has_a_title_in_both_languages() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/app.yml"))
            .expect("the locale file is readable");
        let table: std::collections::BTreeMap<String, serde_yaml_ng::Value> =
            serde_yaml_ng::from_str(&text).expect("the locale file parses");

        for command in ALL {
            let key = command.title_key();
            let entry = table
                .get(&key)
                .unwrap_or_else(|| panic!("missing title for {}", command.id()));
            for language in ["en", "ru"] {
                let text = entry.get(language).and_then(serde_yaml_ng::Value::as_str);
                assert!(
                    text.is_some_and(|text| !text.trim().is_empty()),
                    "missing {language} title for {}",
                    command.id()
                );
            }
        }
    }
}
