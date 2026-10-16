//! tmux control mode on the line of a session: where it starts and ends in the
//! bytes, what is held meanwhile, and what the plain terminal gets back.
//!
//! ```text
//!              ESC P1000p              first line is %begin … 0
//!   Plain ─────────────────► Entering ─────────────────────► Control
//!     ▲  first line another,    │                               │ %exit, detach,
//!     │  held past the buffer,  │                               │ the switch off
//!     │  or the button: given up│                               ▼
//!     └─────────────────────────┘                            Leaving
//!     ▲                                                         │
//!     └──────────────── ESC \, the button, a disconnect ────────┘
//! ```

use zyt_term::TerminalEvent;
use zyt_tmux::Piece;

use super::{Session, Source};
use crate::tmux::{Control, PaneConfig, Phase, Report};

/// What a start of the mode holds until its first line says whether it is
/// tmux.
#[derive(Debug, Default)]
pub struct Entering {
    /// The bytes after `ESC P1000p`.
    held: Vec<u8>,
    /// What was typed meanwhile.
    input: Vec<u8>,
}

/// What the plain terminal stood with before the mode, back after it.
#[derive(Debug, Default)]
pub struct Plain {
    title: Option<String>,
    working_directory: Option<std::path::PathBuf>,
    progress: Option<zyt_term::ProgressState>,
}

/// What the button of tmux shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxLook {
    /// Entering or leaving: an answer is awaited, and this much input held.
    Waiting {
        /// Bytes typed and held.
        held: usize,
        /// Leaving rather than entering.
        leaving: bool,
    },
    /// The mode stands.
    Shown {
        /// The window shown, once it is known.
        window: Option<String>,
        /// How many windows of the session call: a bell or a notification
        /// while not shown.
        calling: usize,
    },
}

/// Bytes held while entering when no read buffer bounds them.
const HELD_WITHOUT_BUFFER: usize = crate::config::DEFAULT_READ_BUFFER * 1024;

impl Session {
    /// Takes a chunk of the line: plain bytes to the terminal, the protocol of
    /// tmux to the mode. With the switch off and no mode standing, the path is
    /// the one of before, byte for byte.
    pub(super) fn take_in(&mut self, chunk: &mut [u8]) {
        if !self.tmux_control
            && !self.splitter.inside()
            && self.entering.is_none()
            && self.tmux.is_none()
        {
            self.terminal.feed(chunk);
            return;
        }
        let mut start = 0;
        while start < chunk.len() {
            let rest = &mut chunk[start..];
            let base = rest.as_ptr() as usize;
            let length = rest.len();
            let mut splitter = std::mem::take(&mut self.splitter);
            let mut restart = None;
            splitter.split(rest, |piece| {
                if restart.is_some() {
                    return;
                }
                match piece {
                    Piece::Plain(bytes) => self.terminal.feed(bytes),
                    Piece::Entered => self.entering = Some(Entering::default()),
                    Piece::Control(bytes) => {
                        let end = end_in_chunk(base, length, bytes);
                        if !self.take_control(bytes) {
                            restart = Some(end);
                        }
                    }
                    Piece::Left => self.left(),
                }
            });
            self.splitter = splitter;
            match restart {
                Some(end) => {
                    self.splitter.reset();
                    start += end;
                }
                None => break,
            }
        }
        self.write_commands();
    }

    /// Bytes inside the mode; `false` when the start was given up and the
    /// rest of the chunk is plain.
    fn take_control(&mut self, bytes: &mut [u8]) -> bool {
        if let Some(control) = &mut self.tmux {
            control.feed(bytes);
            return true;
        }
        let limit = self.read_buffer.unwrap_or(HELD_WITHOUT_BUFFER);
        let Some(entering) = &mut self.entering else {
            self.terminal.feed(bytes);
            return true;
        };
        entering.held.extend_from_slice(bytes);
        match opens_the_mode(&entering.held) {
            Some(true) => {
                self.begin_tmux();
                true
            }
            Some(false) => {
                self.give_up();
                false
            }
            None if entering.held.len() > limit => {
                self.give_up();
                false
            }
            None => true,
        }
    }

    /// The first line was the opening pair of tmux: the mode stands. What was
    /// typed meanwhile was for the shell, and the shell is gone.
    fn begin_tmux(&mut self) {
        let Some(mut entering) = self.entering.take() else {
            return;
        };
        self.plain = Plain {
            title: self.title.clone(),
            working_directory: self.working_directory.clone(),
            progress: self.terminal_progress,
        };
        let lossy = matches!(self.source, Source::Serial { .. });
        let mut control = Control::new(
            lossy,
            self.pane_config(),
            self.area,
            self.read_buffer.unwrap_or(0),
        );
        control.set_pause(self.tmux_pause);
        control.feed(&mut entering.held);
        self.tmux = Some(control);
        log::info!("tmux control mode began");
    }

    /// No tmux after all: what came goes to the plain terminal without
    /// `ESC P1000p`, which would have the emulator swallow what follows, and
    /// what was typed goes to the line. Nothing is written for it.
    fn give_up(&mut self) {
        let Some(entering) = self.entering.take() else {
            return;
        };
        log::info!("the start of tmux control mode was given up");
        self.terminal.feed(&entering.held);
        self.write_line(&entering.input);
    }

    /// `ESC \` after `%exit`: the end of the mode.
    fn left(&mut self) {
        if self.tmux.is_some() {
            self.end_tmux();
        } else {
            self.give_up();
        }
    }

    /// The mode is over: the plain terminal is shown again at the size of the
    /// area, and what was typed while leaving goes to the shell that came back.
    fn end_tmux(&mut self) {
        let Some(mut control) = self.tmux.take() else {
            return;
        };
        let keys = control.take_keys();
        let leaving = control.phase() == Phase::Leaving;
        self.after_tmux();
        if leaving {
            self.write_line(&keys);
        }
        log::info!("tmux control mode ended");
    }

    /// Drops the mode with nothing written: a disconnect by hand.
    pub(super) fn drop_tmux(&mut self) {
        self.entering = None;
        self.splitter.reset();
        if self.tmux.take().is_some() {
            self.after_tmux();
        }
    }

    fn after_tmux(&mut self) {
        let plain = std::mem::take(&mut self.plain);
        self.title = plain.title;
        self.working_directory = plain.working_directory;
        self.terminal_progress = plain.progress;
        self.view_changed = true;
        let (columns, rows) = self.area;
        let _ = self.terminal.resize(columns, rows);
        self.resize(columns, rows);
    }

    /// Input while the mode is entered or stands: held, or sent as
    /// `send-keys`; `false` when it is plain input for the line.
    pub(super) fn hold_input(&mut self, data: &[u8]) -> bool {
        if let Some(entering) = &mut self.entering {
            entering.input.extend_from_slice(data);
            return true;
        }
        if let Some(control) = &mut self.tmux {
            control.input(data);
            self.write_commands();
            return true;
        }
        false
    }

    /// Whether a key typed goes to tmux by its name: in control mode, once it
    /// stands.
    pub fn tmux_takes_names(&self) -> bool {
        self.tmux.is_some() && self.entering.is_none()
    }

    /// Writes a key typed: by `name` to tmux in control mode, which encodes it
    /// for the terminal of its pane; as `bytes`, this terminal's encoding of
    /// it, everywhere else.
    pub fn write_key(&mut self, name: &str, bytes: &[u8]) {
        match &mut self.tmux {
            Some(control) if self.entering.is_none() => {
                control.key(name, bytes);
                self.write_commands();
            }
            _ => self.write(bytes),
        }
    }

    /// Writes what the mode has for the line and passes on what it has to say.
    pub(super) fn write_commands(&mut self) {
        let Some(control) = &mut self.tmux else {
            return;
        };
        let out = control.take_out();
        let told = control.take_told();
        if control.take_shown_changed() {
            self.view_changed = true;
        }
        self.write_line(&out);
        for message in told {
            self.tell(message, Vec::new());
        }
    }

    /// The events of the terminal shown, and in the mode the commands of the
    /// shells of every pane.
    pub(super) fn take_events(&mut self) -> Vec<TerminalEvent> {
        match &mut self.tmux {
            Some(control) => control.take_events(),
            None => self.terminal.take_events(),
        }
    }

    pub(super) fn pane_config(&self) -> PaneConfig {
        PaneConfig {
            scrollback: self.scrollback,
            clipboard: self.clipboard,
            pictures: self.pictures,
            overflow: self.overflow,
        }
    }

    /// How far the screen of a pane may reach past the area, columns and rows.
    /// It is asked every frame and does nothing while the figures stand.
    pub fn set_screen_overflow(&mut self, overflow: (usize, usize)) {
        if self.overflow != overflow {
            self.overflow = overflow;
            self.configure_panes();
        }
    }

    /// `tmux.pause` as it stands: told to tmux at once while the mode stands.
    pub fn set_tmux_pause(&mut self, pause: Option<u32>) {
        if self.tmux_pause == pause {
            return;
        }
        self.tmux_pause = pause;
        if let Some(control) = &mut self.tmux {
            control.set_pause(pause);
            self.write_commands();
        }
    }

    /// How far the screen shown is behind the other end, and whether output
    /// of it was thrown away: what `RX` shows.
    pub fn tmux_lag(&self) -> Option<(std::time::Duration, bool)> {
        self.tmux.as_ref().and_then(Control::lag)
    }

    /// What row −1 of the pane shown says, while tmux holds history above
    /// the oldest line here.
    pub fn tmux_above_top(&self) -> Option<String> {
        self.tmux.as_ref().and_then(Control::above_top)
    }

    /// The next screen of history of the pane shown, from tmux.
    pub fn tmux_load_history(&mut self) {
        if let Some(control) = &mut self.tmux {
            control.load_history();
            self.write_commands();
        }
    }

    /// `RX` pressed: what piled up for the pane shown is skipped.
    pub fn tmux_jump(&mut self) {
        if let Some(control) = &mut self.tmux {
            control.jump_to_now();
            self.write_commands();
        }
    }

    pub(super) fn configure_panes(&mut self) {
        let config = self.pane_config();
        if let Some(control) = &mut self.tmux {
            control.set_config(config);
        }
    }

    /// True once since the terminal shown changed: a pane shown, the mode
    /// over. What the window keeps of the terminal it drew goes with it.
    pub fn take_view_changed(&mut self) -> bool {
        std::mem::take(&mut self.view_changed)
    }

    /// Whether `ESC P1000p` may start the mode. Turned off while the mode
    /// stands, the mode is left with `detach`, as from the menu, rather than
    /// cut: tmux would go on writing its protocol into a terminal that no
    /// longer reads it.
    pub fn set_tmux_control(&mut self, on: bool) {
        self.tmux_control = on;
        if on {
            return;
        }
        if self.entering.is_some() {
            self.give_up();
            self.splitter.reset();
        }
        if let Some(control) = &mut self.tmux {
            control.detach();
            self.write_commands();
        }
    }

    /// The button of tmux was pressed.
    ///
    /// Entering, it gives up: what came is shown as text, what was typed is
    /// sent. Leaving, the plain terminal is back at once, and what more comes
    /// of the protocol reaches it as text. While the mode stands it is the way
    /// to the menu of tmux, which is not there yet.
    pub fn press_tmux(&mut self) {
        if self.entering.is_some() {
            self.give_up();
            self.splitter.reset();
            return;
        }
        if self
            .tmux
            .as_ref()
            .is_some_and(|control| control.phase() == Phase::Leaving)
        {
            self.end_tmux();
            self.splitter.reset();
        }
    }

    /// What the button of tmux shows; `None` without the mode.
    pub fn tmux_look(&self) -> Option<TmuxLook> {
        if let Some(entering) = &self.entering {
            return Some(TmuxLook::Waiting {
                held: entering.input.len(),
                leaving: false,
            });
        }
        let control = self.tmux.as_ref()?;
        Some(match control.phase() {
            Phase::Control => TmuxLook::Shown {
                window: control.label(),
                calling: control.calling(),
            },
            Phase::Leaving => TmuxLook::Waiting {
                held: control.keys_held(),
                leaving: true,
            },
        })
    }

    /// What the menu of tmux is made from; `None` while the mode does not
    /// stand.
    pub fn tmux_menu(&self) -> Option<crate::tmux::Menu> {
        self.tmux
            .as_ref()
            .filter(|control| control.phase() == Phase::Control)
            .map(Control::menu)
    }

    /// Does what was chosen in the menu of tmux.
    pub fn tmux_choose(&mut self, choice: crate::tmux::Choice) {
        if let Some(control) = &mut self.tmux {
            control.choose(choice);
            self.write_commands();
        }
    }

    /// The menu of tmux opened: the lists and the notice are asked again.
    pub fn tmux_refresh(&mut self) {
        if let Some(control) = &mut self.tmux {
            control.refresh();
            self.write_commands();
        }
    }

    /// True once since the lists of tmux came back: a menu of tmux standing is
    /// refilled.
    pub fn take_tmux_lists_changed(&mut self) -> bool {
        self.tmux.as_mut().is_some_and(Control::take_lists_changed)
    }

    /// Whether a pane chosen is on its way while the old one stays shown.
    pub fn tmux_switching(&self) -> bool {
        self.tmux.as_ref().is_some_and(Control::switching)
    }

    /// What the debug window says of the mode; `None` without it.
    pub fn tmux_report(&self) -> Option<Report> {
        self.tmux.as_ref().map(Control::report)
    }

    /// Whether tmux sets the size of what is shown: the view asks it for a
    /// size and draws whatever size it gives.
    pub fn tmux_sets_size(&self) -> bool {
        self.tmux.is_some()
    }
}

/// Where a piece ends in the chunk it was cut from; past the chunk when it is
/// a piece the splitter held back, which is all of it.
fn end_in_chunk(base: usize, length: usize, piece: &[u8]) -> usize {
    let start = piece.as_ptr() as usize;
    match start.checked_sub(base) {
        Some(offset) if offset + piece.len() <= length => offset + piece.len(),
        _ => length,
    }
}

/// Whether the first line after `ESC P1000p` is the opening pair of tmux:
/// `%begin`, three numbers, flags with no `1` in them. `None` until the line
/// is whole.
fn opens_the_mode(held: &[u8]) -> Option<bool> {
    let end = memchr::memchr(b'\n', held)?;
    let line = held[..end].strip_suffix(b"\r").unwrap_or(&held[..end]);
    let words: Vec<&[u8]> = line.split(|&byte| byte == b' ').collect();
    let opening = match words.as_slice() {
        [b"%begin", time, number, flags] => {
            let numbers = [time, number, flags]
                .iter()
                .all(|word| !word.is_empty() && word.iter().all(u8::is_ascii_digit));
            numbers && flags.iter().all(|&digit| digit == b'0')
        }
        _ => false,
    };
    Some(opening)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_opening_pair_is_known_by_its_first_line() {
        assert_eq!(opens_the_mode(b"%begin 1792 260 0\r\n%end"), Some(true));
        assert_eq!(opens_the_mode(b"%begin 1792 260 1\r\n"), Some(false));
        assert_eq!(opens_the_mode(b"hello\r\n"), Some(false));
        assert_eq!(opens_the_mode(b"%begin 1 2\r\n"), Some(false));
        assert_eq!(opens_the_mode(b"%begin 1792 2"), None);
    }

    #[test]
    fn a_piece_held_back_ends_the_chunk() {
        let chunk = [0u8; 10];
        let base = chunk.as_ptr() as usize;
        assert_eq!(end_in_chunk(base, 10, &chunk[2..5]), 5);
        assert_eq!(end_in_chunk(base, 10, b"\x1b"), 10);
    }
}
