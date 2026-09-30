//! The hold this side keeps on the reading of a console.
//!
//! It is a state and not an action: the reading stops when it is set and begins
//! again when it is cleared, and the thread that reads waits on it meanwhile.
//! Nothing is thrown away by it — the pipe of the pseudo terminal fills behind
//! the reading, and the program writing into it waits at its next write, which is
//! what a full read buffer already does. The difference is only who asked.
//!
//! The wait is a condition variable and not a sleep between two looks: a console
//! held for an hour costs no wakeups at all, and the reading begins again on the
//! call that clears it rather than on the next look.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Whether the reading is held, and the wait of the thread that reads.
#[derive(Debug, Default)]
pub struct ReadHold {
    held: Mutex<bool>,
    freed: Condvar,
}

impl ReadHold {
    /// A hold nobody has set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds the reading, or lets it go.
    ///
    /// The thread that reads is woken as the hold is let go of, so nothing waits
    /// for a look that was going to happen anyway.
    pub fn set(&self, held: bool) {
        *self.held.lock().unwrap_or_else(|error| error.into_inner()) = held;
        if !held {
            self.freed.notify_all();
        }
    }

    /// Whether the reading is held.
    pub fn held(&self) -> bool {
        *self.held.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Waits while the reading is held, and answers whether it still is.
    ///
    /// It comes back as the hold is let go of, and after `timeout` whether it was
    /// or not: a caller that has another reason to look — a session that is
    /// ending — is a caller that must not be held here for ever.
    pub fn wait(&self, timeout: Duration) -> bool {
        let held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        if !*held {
            return false;
        }
        let (held, _) = self
            .freed
            .wait_timeout(held, timeout)
            .unwrap_or_else(|error| error.into_inner());
        *held
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_hold_nobody_set_holds_nothing_and_waits_for_nothing() {
        let hold = ReadHold::new();

        assert!(!hold.held());
        assert!(
            !hold.wait(Duration::from_secs(30)),
            "and it comes back at once"
        );
    }

    #[test]
    fn a_wait_ends_as_the_hold_is_let_go_of() {
        let hold = Arc::new(ReadHold::new());
        hold.set(true);
        assert!(hold.held());

        let waiting = {
            let hold = hold.clone();
            std::thread::spawn(move || hold.wait(Duration::from_secs(30)))
        };
        std::thread::sleep(Duration::from_millis(20));
        hold.set(false);

        assert!(
            !waiting.join().expect("the waiting thread ends"),
            "the wait answers that the reading is free"
        );
    }

    #[test]
    fn a_hold_nobody_lets_go_of_comes_back_after_the_wait() {
        let hold = ReadHold::new();
        hold.set(true);

        assert!(hold.wait(Duration::from_millis(10)), "still held");
    }
}
