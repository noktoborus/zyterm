//! Whether tmux answers: the commands held while it is silent, and what came
//! on the line once something else speaks there (`docs/tmux.md`, *When tmux
//! does not answer*).

use std::time::{Duration, Instant};

use zyt_tmux::Command;

use super::Control;

/// What is watched of the answers of tmux.
#[derive(Debug)]
pub(super) struct Answers {
    /// How long tmux may be silent with a command unanswered.
    after: Duration,
    /// The time of the last turn.
    now: Instant,
    /// Lines of the protocol read, as last seen.
    heard: u64,
    /// Since when a command waits with nothing heard, the clock running.
    since: Option<Instant>,
    /// The clock runs: the line is read, and what was written left this side.
    counting: bool,
    /// tmux is taken as silent: commands are held.
    silent: bool,
    /// What came on the line once it was no longer tmux.
    gone: Option<Vec<u8>>,
}

impl Answers {
    pub(super) fn new() -> Self {
        Self {
            after: Duration::from_secs(u64::from(crate::config::DEFAULT_UNANSWERED_AFTER)),
            now: Instant::now(),
            heard: 0,
            since: None,
            counting: true,
            silent: false,
            gone: None,
        }
    }

    /// tmux is taken as silent: nothing is written, and the menu is not
    /// offered.
    pub(super) fn silent(&self) -> bool {
        self.silent
    }
}

impl Control {
    /// How long tmux may leave a command unanswered, with nothing at all
    /// heard from it, before it is taken as silent.
    pub fn set_unanswered_after(&mut self, after: Duration) {
        self.answers.after = after;
        self.watch();
    }

    /// The time of a turn of the screen, and whether the clock runs: not
    /// while the window holds the line for a selection, nor while the reading
    /// is held, nor while what was written has not left this side — tmux
    /// cannot answer what it was not given.
    pub fn tick(&mut self, now: Instant, counting: bool) {
        self.answers.now = now;
        self.answers.counting = counting;
        self.watch();
    }

    /// How long tmux has been silent with a command unanswered, once it is
    /// taken as silent.
    pub fn unanswered(&self) -> Option<Duration> {
        let since = self.answers.since.filter(|_| self.answers.silent)?;
        Some(self.answers.now.saturating_duration_since(since))
    }

    /// What came on the line from the first line that was not of tmux on: the
    /// mode is over, and it belongs to the plain terminal.
    pub fn take_gone(&mut self) -> Option<Vec<u8>> {
        self.answers.gone.take()
    }

    /// A line that was not of tmux, from the head of it held before to the
    /// end of the chunk.
    pub(super) fn lose_tmux(&mut self, held: Vec<u8>, rest: &[u8]) {
        let mut gone = held;
        gone.extend_from_slice(rest);
        self.answers.gone = Some(gone);
    }

    /// The mode is let go of while tmux is silent: what was held is thrown
    /// away, and the line gets `detach` alone. A tmux that was only slow
    /// leaves on it; a shell that came back in its place reads one line it
    /// does not know.
    pub fn abandon(&mut self) -> Vec<u8> {
        self.out.clear();
        self.take_keys();
        self.beside_keys.clear();
        self.client.forget();
        let mut out = Vec::new();
        self.client.send(Command::Detach, &mut out);
        self.client.forget();
        out
    }

    /// Looks at the clock: a line of tmux heard restarts it, a command
    /// unanswered past the time takes tmux as silent, nothing awaited stops
    /// it.
    pub(super) fn watch(&mut self) {
        let heard = self.client.heard();
        let answers = &mut self.answers;
        let spoke = heard != answers.heard;
        answers.heard = heard;
        if spoke {
            answers.silent = false;
        }
        if self.client.sent().len() == 0 {
            answers.since = None;
            answers.silent = false;
            return;
        }
        let since = match answers.since {
            Some(since) if !spoke && answers.counting && !self.line_gone => since,
            _ => {
                answers.since = Some(answers.now);
                return;
            }
        };
        if answers.now.saturating_duration_since(since) >= answers.after {
            answers.silent = true;
        }
    }
}

#[cfg(test)]
mod tests;
