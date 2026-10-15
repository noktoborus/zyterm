//! Flow control: `pause-after` asked of tmux, the lag `RX` shows, and a pane
//! tmux paused let go under a gap.

use std::time::Duration;

use rust_i18n::t;
use zyt_tmux::{Command, PaneId, Part};

use super::{Control, LAG_SHOWN, Phase, SUBSCRIPTIONS};

impl Control {
    /// `tmux.pause` as it stands: `pause-after` is sent, or taken back, at
    /// once while the mode stands; at the start otherwise.
    pub fn set_pause(&mut self, pause: Option<u32>) {
        self.pause = pause;
        let standing = self.state.phase == Phase::Control
            && !self.line_gone
            && self.state.serves(SUBSCRIPTIONS);
        if standing && self.pause_told != pause {
            self.client.send(Command::PauseAfter(pause), &mut self.out);
            self.pause_told = pause;
        }
    }

    /// How far the screen shown is behind the other end, and whether tmux
    /// threw output of it away: what `RX` shows. Only with `pause-after`
    /// asked, since tmux says no age otherwise, and from a second behind.
    pub fn lag(&self) -> Option<(Duration, bool)> {
        self.pause_told?;
        let age = self
            .state
            .shown
            .and_then(|pane| self.state.panes.get(&pane))
            .and_then(|pane| pane.age)?;
        (age >= LAG_SHOWN).then_some((age, self.state.lost))
    }

    /// `RX` pressed: what piled up for the pane shown is thrown away
    /// (`refresh-client -A '%P:pause'`); the `%pause` it brings lets the pane
    /// go and snapshots it under a gap.
    pub fn jump_to_now(&mut self) {
        let Some(pane) = self.state.shown else {
            return;
        };
        if self.state.phase == Phase::Control && !self.line_gone {
            self.client.send(
                Command::PaneFlow {
                    pane,
                    flow: zyt_tmux::Flow::Pause,
                },
                &mut self.out,
            );
        }
    }

    /// A pane tmux paused: let go and snapshotted over the terminal it has,
    /// the line of the gap between what it held and the screen of now.
    pub(super) fn resume(&mut self, id: PaneId) {
        let (columns, rows) = self.state.size_of(id).unwrap_or(self.size);
        let Ok(fresh) = self.state.make_terminal(columns, rows) else {
            return;
        };
        let Some(pane) = self.state.panes.get_mut(&id) else {
            return;
        };
        let skipped = pane
            .paused
            .take()
            .map_or(Duration::ZERO, |paused| paused.skipped());
        let text = t!("tmux.gap", seconds = skipped.as_secs().max(1)).to_string();
        pane.begin_gap(&text, fresh);
        pane.tries += 1;
        self.client.send_line(
            &[
                Command::PaneFlow {
                    pane: id,
                    flow: zyt_tmux::Flow::Continue,
                },
                Command::Display(id),
                Command::Capture {
                    pane: id,
                    part: Part::Saved,
                },
                Command::Capture {
                    pane: id,
                    part: Part::Visible,
                },
            ],
            &mut self.out,
        );
    }
}
