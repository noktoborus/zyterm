//! History of a pane loaded on demand: row −1 above the oldest line offers
//! what tmux keeps further up, and a snapshot with a depth takes it.
//!
//! Lines are never put above the oldest one here — the emulator cannot — so
//! the pane is snapshotted again, whole and deeper: `display -p` first, since
//! what to capture depends on the alternate screen, then one range of history
//! and screen off it, or history, the normal screen and the alternate one on
//! it. The terminal is made anew and the view is kept on its line of text.

use rust_i18n::t;
use zyt_tmux::{Command, PaneId, Part};

use super::pane::PaneState;
use super::{Control, Phase};

impl Control {
    /// What row −1 of the pane shown says: the offer of the next screen of
    /// history while tmux holds lines above the oldest one here, or that it is
    /// on its way. `None` while there is nothing above.
    pub fn above_top(&self) -> Option<String> {
        let pane = self.state.panes.get(&self.state.shown?)?;
        let rows = pane.terminal.size().1 as u32;
        if pane.deeper.is_some() || (pane.state == PaneState::Loading && pane.depth > 0) {
            return Some(t!("tmux.loading_history").to_string());
        }
        if pane.state != PaneState::Fresh {
            return None;
        }
        let left = pane.tmux_history().saturating_sub(pane.depth);
        (left > 0).then(|| t!("tmux.load_history", lines = left.min(rows)).to_string())
    }

    /// The next screen of history of the pane shown: its modes are asked, and
    /// their reply asks the captures (`deep_snapshot`).
    pub fn load_history(&mut self) {
        if self.state.phase != Phase::Control || self.line_gone {
            return;
        }
        let Some(id) = self.state.shown else {
            return;
        };
        let Some(pane) = self.state.panes.get_mut(&id) else {
            return;
        };
        if pane.state != PaneState::Fresh || pane.deeper.is_some() {
            return;
        }
        let rows = pane.terminal.size().1 as u32;
        let here = pane.terminal.history_size() as u32;
        let left = pane.tmux_history().saturating_sub(pane.depth);
        if left == 0 {
            return;
        }
        pane.deeper = Some(here + left.min(rows));
        pane.deep_modes = None;
        self.client.send(Command::Display(id), &mut self.out);
    }

    /// The modes of a deeper snapshot came: the terminal is made anew and the
    /// history with the screens is asked, as deep as tmux holds.
    pub(super) fn deep_snapshot(&mut self, id: PaneId) {
        let Some(pane) = self.state.panes.get_mut(&id) else {
            return;
        };
        let (Some(wanted), Some(modes)) = (pane.deeper.take(), pane.deep_modes.take()) else {
            return;
        };
        let depth = wanted.min(modes.history);
        let alternate = modes.alternate;
        let (columns, rows) = (modes.columns as usize, modes.rows as usize);
        let Ok(terminal) = self.state.make_terminal(columns, rows) else {
            return;
        };
        let Some(pane) = self.state.panes.get_mut(&id) else {
            return;
        };
        pane.begin_deep(terminal, modes, depth);
        let commands: Vec<Command> = match alternate {
            false => vec![Command::Capture {
                pane: id,
                part: Part::Range { depth },
            }],
            true => vec![
                Command::Capture {
                    pane: id,
                    part: Part::History { depth },
                },
                Command::Capture {
                    pane: id,
                    part: Part::Saved,
                },
                Command::Capture {
                    pane: id,
                    part: Part::Visible,
                },
            ],
        };
        self.client.send_line(&commands, &mut self.out);
    }
}
