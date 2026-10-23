//! The screen of a session: the terminal, fed in a thread of its own.
//!
//! ```text
//!  source ──rx──► screen thread ──feed──► Terminal ──snapshot──► board ──swap──► window
//!    ▲                 │  ▲                                                       │
//!    └──tx── answers ──┘  └────────────── ScreenCommand (channel) ◄───────────────┘
//! ```
//!
//! The window never waits for the screen: it sends commands, and takes what
//! the board holds when it draws. The screen never waits for the window: it
//! feeds the terminal as the bytes come, frames or not.

mod board;
mod command;
mod wake;
mod wire;
mod worker;

#[cfg(test)]
mod tests;

pub use board::{Said, Told};
pub use command::{ScreenCommand, ScreenSettings, TmuxCommand};
pub use worker::Setup;
pub use worker::TmuxLook;

use board::Board;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
pub use wake::Wake;
pub use wire::Wire;
use worker::Screen;

/// The side of the screen the window holds.
///
/// Dropped, it asks the thread to stop and waits for nothing.
pub struct ScreenHandle {
    commands: Sender<ScreenCommand>,
    board: Arc<Mutex<Board>>,
    wake: Arc<Wake>,
    turning: Arc<Mutex<Option<std::time::Instant>>>,
}

impl ScreenHandle {
    /// Starts the thread of a screen. `shown` is called when the board holds
    /// something new, from that thread.
    ///
    /// The screen is made in its thread and never leaves it: a terminal keeps
    /// what it reports in a cell that is not shared between threads.
    pub fn spawn(setup: Setup, shown: Arc<dyn Fn() + Send + Sync>) -> crate::error::Result<Self> {
        let (commands, received) = channel();
        let board = Arc::new(Mutex::new(Board::default()));
        let wake = Arc::new(Wake::default());
        let (started, start) = channel();
        let turning = Arc::new(Mutex::new(None));
        let (shared, rung, marked) = (board.clone(), wake.clone(), turning.clone());
        std::thread::Builder::new()
            .name("zyterm-screen".to_string())
            .spawn(
                move || match Screen::new(setup, received, shared, rung, shown, marked) {
                    Ok(screen) => {
                        let _ = started.send(Ok(()));
                        screen.run();
                    }
                    Err(error) => {
                        let _ = started.send(Err(error));
                    }
                },
            )
            .map_err(|source| crate::error::AppError::Thread { source })?;
        start.recv().map_err(|_| crate::error::AppError::Thread {
            source: std::io::Error::other("the screen ended before it started"),
        })??;
        Ok(Self {
            commands,
            board,
            wake,
            turning,
        })
    }

    /// Sends one command; the screen applies it on its next turn.
    pub fn send(&self, command: ScreenCommand) {
        if self.commands.send(command).is_ok() {
            self.wake.ring();
        }
    }

    /// How long the turn under way has taken; nothing between two turns.
    pub fn busy_for(&self) -> Option<std::time::Duration> {
        self.turning
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .map(|since| since.elapsed())
    }

    /// What sends a command to the screen, from any thread.
    pub fn sender(&self) -> impl Fn(ScreenCommand) + Send + Sync + 'static {
        let (commands, wake) = (self.commands.clone(), self.wake.clone());
        move |command| {
            if commands.send(command).is_ok() {
                wake.ring();
            }
        }
    }

    /// What wakes the screen: for a source to call when bytes arrive.
    pub fn waker(&self) -> Arc<dyn Fn() + Send + Sync> {
        let wake = self.wake.clone();
        Arc::new(move || wake.ring())
    }

    /// What reads the state of the terminal off the board, from any thread.
    pub fn state_reader(
        &self,
    ) -> Arc<dyn Fn() -> Option<(zyt_term::TermState, u64)> + Send + Sync> {
        let board = self.board.clone();
        Arc::new(move || {
            board
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .state()
        })
    }

    /// Takes what the board holds into `view`.
    pub fn take(&self, view: &mut View) {
        let (page, revision) = view.snapshot.parts_mut();
        self.board
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take(page, revision, &mut view.said, &mut view.told);
    }
}

impl Drop for ScreenHandle {
    fn drop(&mut self) {
        self.send(ScreenCommand::Stop);
    }
}

/// What the window holds of the screen between two takes.
#[derive(Debug, Default)]
pub struct View {
    /// The page, as the widget draws it.
    pub snapshot: zyt_term::Snapshot,
    /// The values the terminal said last.
    pub said: Said,
    /// What was told since the window last acted on it.
    pub told: Told,
}
