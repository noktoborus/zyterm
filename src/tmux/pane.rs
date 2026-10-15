//! One pane of tmux that has a terminal here, and the restore of its snapshot.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use zyt_term::Terminal;
use zyt_tmux::{Modes, Piece, Splitter};

/// Where a pane with a terminal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneState {
    /// Its snapshot was asked for; its output is dropped until the snapshot
    /// ends, since the snapshot holds it.
    Loading,
    /// Its terminal is what tmux holds; its output is fed.
    Fresh,
    /// Made by its output, never snapshotted: what it says — the bell, a
    /// notification, a title, a command marked — is read from what comes,
    /// while its screen holds only that. Shown, it is snapshotted first.
    Unseen,
}

/// A pane whose output came or that was shown: a terminal of its own.
pub struct Pane {
    /// The emulator of the pane.
    pub terminal: Terminal,
    /// Watches its output for control mode started inside it.
    splitter: Splitter,
    /// Output and restore of this frame, fed at the end of the chunk.
    inbox: Vec<u8>,
    /// Where it stands.
    pub state: PaneState,
    /// The modes of its last snapshot, from `display -p`.
    modes: Option<Modes>,
    /// Lines of the screen of the reply being read.
    lines: usize,
    /// A command of the snapshot failed or was cut short.
    pub snapshot_failed: bool,
    /// Snapshots asked for this pane in a row without one ending well.
    pub tries: u32,
    /// The size tmux gave the pane.
    asked: (usize, usize),
    /// How large its screen may be: the area and the margin past it.
    limit: (usize, usize),
    /// How old its last output was when it came, from `%extended-output`;
    /// none from `%output`, which says no age.
    pub age: Option<Duration>,
    /// tmux paused it: when, and how far behind it was. What was queued for
    /// it is thrown away until it is let go.
    pub paused: Option<Paused>,
    /// Lines of history above the screen the snapshot of this terminal took.
    pub depth: u32,
    /// A deeper snapshot asked for, this many lines of history: its modes are
    /// on their way, and the terminal stands as it is until they come.
    pub deeper: Option<u32>,
    /// The modes of the deeper snapshot, before its captures are asked.
    pub deep_modes: Option<Modes>,
    /// The terminal shown while a deeper snapshot is written into a new one:
    /// the screen stands as it was until the new one is whole.
    pub standing: Option<Terminal>,
    /// Where the view stood when a deeper snapshot began, lines from the
    /// bottom: the lines loaded come above, so it stands on the same line of
    /// text after.
    pub keep_view: Option<usize>,
}

/// A pane tmux paused because it fell behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused {
    /// When `%pause` came.
    pub at: Instant,
    /// How far behind it was then.
    pub behind: Duration,
}

impl Paused {
    /// How much output was skipped: what was behind, and what came while it
    /// stood paused.
    pub fn skipped(&self) -> Duration {
        self.behind + self.at.elapsed()
    }
}

/// The modes a snapshot only turns on, turned off before a snapshot is written
/// over a terminal that is kept: a program gone from the alternate screen may
/// have left any of them on.
const MODES_OFF: &[u8] =
    b"\x1b[?1l\x1b>\x1b[4l\x1b[?7h\x1b[?25h\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1005l";

/// What the output of a pane brought besides bytes for its terminal.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    /// Bytes for the terminal, or nothing.
    Plain,
    /// `ESC P1000p` inside the pane: control mode in control mode.
    Nested,
}

impl Pane {
    /// A pane whose snapshot is about to be asked for, its screen held to
    /// `limit`.
    pub fn loading(terminal: Terminal, limit: (usize, usize)) -> Self {
        Self::made(terminal, limit, PaneState::Loading)
    }

    /// A pane made by its output, before any snapshot.
    pub fn unseen(terminal: Terminal, limit: (usize, usize)) -> Self {
        Self::made(terminal, limit, PaneState::Unseen)
    }

    fn made(terminal: Terminal, limit: (usize, usize), state: PaneState) -> Self {
        let asked = terminal.size();
        let mut pane = Self {
            terminal,
            splitter: Splitter::new(),
            inbox: Vec::new(),
            state,
            modes: None,
            lines: 0,
            snapshot_failed: false,
            tries: 0,
            asked,
            limit,
            age: None,
            paused: None,
            depth: 0,
            deeper: None,
            deep_modes: None,
            standing: None,
            keep_view: None,
        };
        pane.apply_size();
        pane
    }

    /// Output of the pane from `%output`. While `Loading` it is dropped; a
    /// control mode started inside the pane is cut out of what is fed.
    pub fn output(&mut self, bytes: &[u8]) -> Output {
        let mut nested = false;
        let fresh = self.state != PaneState::Loading;
        let inbox = &mut self.inbox;
        self.splitter
            .split(&mut bytes.to_vec(), |piece| match piece {
                Piece::Plain(plain) if fresh => inbox.extend_from_slice(plain),
                Piece::Entered => nested = true,
                Piece::Plain(_) | Piece::Control(_) | Piece::Left => {}
            });
        match nested {
            true => Output::Nested,
            false => Output::Plain,
        }
    }

    /// Feeds what the chunk brought to the terminal; its answers are dropped,
    /// since tmux already answered the program.
    pub fn flush(&mut self) {
        if self.inbox.is_empty() {
            return;
        }
        self.terminal.feed(&self.inbox);
        self.inbox.clear();
        self.terminal.forget_output();
        if self.state == PaneState::Fresh
            && let Some(view) = self.keep_view.take()
        {
            self.terminal.scroll_to(view);
            self.standing = None;
        }
    }

    /// The size of the pane, as a layout or a snapshot gives it, held to the
    /// limit: what is past it is cut, and tmux is not told.
    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.asked = (columns, rows);
        self.apply_size();
    }

    /// The limit moved with the area: the screen is cut further, or given back
    /// what was cut.
    pub fn set_limit(&mut self, limit: (usize, usize)) {
        self.limit = limit;
        self.apply_size();
    }

    fn apply_size(&mut self) {
        let columns = self.asked.0.min(self.limit.0).max(2);
        let rows = self.asked.1.min(self.limit.1).max(1);
        if self.terminal.size() != (columns, rows) {
            let _ = self.terminal.resize(columns, rows);
        }
    }

    /// Starts a snapshot of a pane that was paused, over the terminal it has:
    /// the screen as it stood goes up into the scrollback with the line of the
    /// gap under it (`Terminal::mark_gap`), and the snapshot is written below.
    /// On the alternate screen, which has no scrollback, `fresh` is the
    /// terminal the snapshot is written on instead.
    pub fn begin_gap(&mut self, text: &str, fresh: Terminal) {
        if self.terminal.modes().alt_screen {
            self.begin_snapshot(fresh);
            return;
        }
        self.terminal.feed(MODES_OFF);
        self.terminal.mark_gap(text);
        self.terminal.forget_output();
        self.inbox.clear();
        self.state = PaneState::Loading;
        self.modes = None;
        self.lines = 0;
        self.snapshot_failed = false;
        self.splitter = Splitter::new();
    }

    /// The terminal to show: the one standing while a deeper snapshot is
    /// written, this one otherwise.
    pub fn view(&self) -> &Terminal {
        self.standing.as_ref().unwrap_or(&self.terminal)
    }

    /// The same, to change.
    pub fn view_mut(&mut self) -> &mut Terminal {
        self.standing.as_mut().unwrap_or(&mut self.terminal)
    }

    /// Lines of history tmux held above the screen at the last snapshot.
    pub fn tmux_history(&self) -> u32 {
        self.modes.as_ref().map_or(0, |modes| modes.history)
    }

    /// Starts a snapshot `depth` lines deep on a terminal made anew, with the
    /// modes its `display -p` gave: the history comes above the screen, and
    /// the view is kept on its line of text.
    pub fn begin_deep(&mut self, terminal: Terminal, modes: Modes, depth: u32) {
        let view = self.terminal.display_offset();
        let standing = std::mem::replace(&mut self.terminal, terminal);
        self.restart();
        self.standing = Some(standing);
        self.depth = depth;
        self.deeper = None;
        self.keep_view = Some(view);
        self.resize(modes.columns as usize, modes.rows as usize);
        self.modes = Some(modes);
    }

    /// Starts a snapshot of the pane on a terminal that holds nothing yet.
    pub fn begin_snapshot(&mut self, terminal: Terminal) {
        self.terminal = terminal;
        self.restart();
    }

    /// What a snapshot starts from, on the terminal the pane now has.
    fn restart(&mut self) {
        self.standing = None;
        self.depth = 0;
        self.deep_modes = None;
        self.asked = self.terminal.size();
        self.apply_size();
        self.inbox.clear();
        self.state = PaneState::Loading;
        self.modes = None;
        self.lines = 0;
        self.snapshot_failed = false;
        self.splitter = Splitter::new();
    }

    /// The reply of `display -p`: the size first, since the lines of a
    /// snapshot are laid out for one width. `false` when it is not the modes.
    pub fn take_modes(&mut self, line: &[u8]) -> bool {
        match Modes::parse(line) {
            Ok(modes) if self.deeper.is_some() && self.state != PaneState::Loading => {
                self.deep_modes = Some(modes);
                true
            }
            Ok(modes) => {
                self.resize(modes.columns as usize, modes.rows as usize);
                self.modes = Some(modes);
                true
            }
            Err(error) => {
                log::debug!("tmux: the modes of a pane: {error}");
                false
            }
        }
    }

    /// One line of a screen of the snapshot. `false` when the screen is longer
    /// than the pane: the `%end` of the reply was lost.
    pub fn screen_line(&mut self, line: &[u8]) -> bool {
        let rows = self.modes.as_ref().map_or(0, |modes| modes.rows as usize);
        self.lines += 1;
        if self.lines > rows + self.depth as usize {
            return false;
        }
        match self.lines {
            1 => self.inbox.extend_from_slice(b"\x1b[H"),
            _ => self.inbox.extend_from_slice(b"\r\n"),
        }
        self.inbox.extend_from_slice(line);
        self.inbox.extend_from_slice(b"\x1b[m");
        true
    }

    /// The end of the screen under the alternate one: the cursor the program
    /// left there is put back, and `?1049h` saves it as the program's own did,
    /// so `?1049l` brings that screen back when the program leaves.
    pub fn saved_screen_end(&mut self) {
        self.lines = 0;
        let Some(modes) = &self.modes else {
            return;
        };
        if !modes.alternate {
            return;
        }
        if let Some((x, y)) = modes.alternate_saved {
            let _ = write!(Sequence(&mut self.inbox), "\x1b[{};{}H", y + 1, x + 1);
        }
        self.inbox.extend_from_slice(b"\x1b[?1049h");
    }

    /// The end of the visible screen: the modes and the cursor, and the pane
    /// is fresh.
    pub fn visible_screen_end(&mut self) {
        self.lines = 0;
        if let Some(modes) = &self.modes {
            write_modes(modes, &mut self.inbox);
        }
        self.state = PaneState::Fresh;
    }
}

/// Writes into a byte buffer through `fmt::Write`.
struct Sequence<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Sequence<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.extend_from_slice(text.as_bytes());
        Ok(())
    }
}

/// The modes `capture-pane` does not carry, after the screens and before the
/// cursor: `ESC[r` moves the cursor home, and `?7l` and `4h` would change how
/// the screens are written.
pub fn write_modes(modes: &Modes, out: &mut Vec<u8>) {
    let mut out = Sequence(out);
    let (upper, lower) = modes.scroll_region;
    if (upper, lower) != (0, modes.rows - 1) {
        let _ = write!(out, "\x1b[{};{}r", upper + 1, lower + 1);
    }
    let switches = [
        (!modes.wrap, "\x1b[?7l"),
        (modes.insert, "\x1b[4h"),
        (modes.keypad_cursor, "\x1b[?1h"),
        (modes.keypad, "\x1b="),
        (!modes.cursor_visible, "\x1b[?25l"),
        (modes.mouse_standard, "\x1b[?1000h"),
        (modes.mouse_button, "\x1b[?1002h"),
        (modes.mouse_all, "\x1b[?1003h"),
        (modes.mouse_sgr, "\x1b[?1006h"),
        (modes.mouse_utf8, "\x1b[?1005h"),
    ];
    for (on, sequence) in switches {
        if on {
            let _ = out.write_str(sequence);
        }
    }
    let _ = out.write_str("\x1b[3g");
    for stop in &modes.tabs {
        let _ = write!(out, "\x1b[1;{}H\x1bH", stop + 1);
    }
    let (x, y) = modes.cursor;
    if modes.origin {
        let _ = write!(
            out,
            "\x1b[?6h\x1b[{};{}H",
            y.saturating_sub(upper) + 1,
            x + 1
        );
    } else {
        let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(columns: usize, rows: usize) -> Terminal {
        Terminal::new(zyt_term::TerminalConfig {
            columns,
            rows,
            scrollback: 100,
            clipboard: zyt_term::ClipboardAccess::default(),
        })
        .expect("a terminal")
    }

    fn text(terminal: &mut Terminal) -> Vec<String> {
        let content = terminal.content();
        (0..content.rows)
            .map(|row| {
                (0..content.columns)
                    .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    const NO_LIMIT: (usize, usize) = (usize::MAX, usize::MAX);

    #[test]
    fn a_screen_past_the_limit_is_cut_and_given_back() {
        let mut pane = Pane::loading(terminal(80, 24), (100, 30));
        pane.resize(5000, 100);
        assert_eq!(pane.terminal.size(), (100, 30));
        pane.set_limit((520, 240));
        assert_eq!(pane.terminal.size(), (520, 100), "what was cut comes back");
        pane.set_limit((60, 20));
        assert_eq!(pane.terminal.size(), (60, 20));
    }

    const MODES_OF_A_SHELL: &[u8] =
        b"20 4 0 0 4294967295 4294967295 2 1 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16";

    #[test]
    fn a_screen_is_written_and_the_cursor_put_back() {
        let mut pane = Pane::loading(terminal(80, 24), NO_LIMIT);
        assert!(pane.take_modes(MODES_OF_A_SHELL));
        assert_eq!(pane.terminal.size(), (20, 4), "the size comes first");
        pane.saved_screen_end();
        for line in [&b"$ ls"[..], b"a b", b"\x1b[31mred\x1b[0m", b""] {
            assert!(pane.screen_line(line));
        }
        assert!(
            !pane.screen_line(b"one too many"),
            "a fifth line of four rows"
        );
        pane.visible_screen_end();
        pane.flush();
        assert_eq!(pane.state, PaneState::Fresh);
        assert_eq!(text(&mut pane.terminal), ["$ ls", "a b", "red", ""]);
        let cursor = pane.terminal.content().cursor.expect("the cursor is shown");
        assert_eq!((cursor.column, cursor.row), (2, 1));
    }

    #[test]
    fn the_screen_under_the_alternate_one_comes_back_when_the_program_leaves() {
        let mut pane = Pane::loading(terminal(80, 24), NO_LIMIT);
        assert!(pane.take_modes(b"20 4 0 1 0 2 0 0 0 3 0 1 0 0 0 0 0 1 0 1 0 8,16"));
        assert!(pane.screen_line(b"$ vim"));
        assert!(pane.screen_line(b"before"));
        pane.saved_screen_end();
        assert!(pane.screen_line(b"~ in vim"));
        pane.visible_screen_end();
        pane.flush();
        assert_eq!(text(&mut pane.terminal)[0], "~ in vim");
        assert!(
            pane.terminal.modes().mouse_report,
            "the mouse of the program"
        );
        pane.terminal.feed(b"\x1b[?1049l");
        assert_eq!(text(&mut pane.terminal)[..2], ["$ vim", "before"]);
    }

    #[test]
    fn output_is_dropped_while_loading_and_fed_once_fresh() {
        let mut pane = Pane::loading(terminal(20, 4), NO_LIMIT);
        assert_eq!(pane.output(b"lost"), Output::Plain);
        assert!(pane.take_modes(MODES_OF_A_SHELL));
        pane.visible_screen_end();
        assert_eq!(pane.output(b"kept"), Output::Plain);
        pane.flush();
        assert!(text(&mut pane.terminal).concat().contains("kept"));
        assert!(!text(&mut pane.terminal).concat().contains("lost"));
    }

    #[test]
    fn control_mode_inside_a_pane_is_cut_out() {
        let mut pane = Pane::loading(terminal(20, 4), NO_LIMIT);
        assert!(pane.take_modes(MODES_OF_A_SHELL));
        pane.visible_screen_end();
        assert_eq!(
            pane.output(b"before\x1bP1000p%begin 1 1 0\r\n"),
            Output::Nested
        );
        pane.flush();
        let shown = text(&mut pane.terminal).concat();
        assert!(
            shown.contains("before") && !shown.contains("%begin"),
            "{shown}"
        );
    }

    #[test]
    fn modes_are_written_as_the_program_set_them() {
        let modes = Modes::parse(b"80 24 0 0 0 0 5 7 4 9 1 0 1 1 1 0 1 0 0 1 0 4,10").unwrap();
        let mut out = Vec::new();
        write_modes(&modes, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "\x1b[5;10r\x1b[?7l\x1b[4h\x1b[?1h\x1b=\x1b[?25l\x1b[?1000h\x1b[?1006h\
             \x1b[3g\x1b[1;5H\x1bH\x1b[1;11H\x1bH\x1b[?6h\x1b[4;6H"
        );
    }
}
