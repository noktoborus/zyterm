//! The one wait of the screen.
//!
//! Bytes of the line, a command of the window and a portion of a script all
//! wake it through the same signal, so it waits in one place for all of them.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// A signal that is kept until it is waited for.
#[derive(Debug, Default)]
pub struct Wake {
    rung: Mutex<bool>,
    woken: Condvar,
}

impl Wake {
    /// Wakes the waiting thread, or the next wait if none is waiting.
    pub fn ring(&self) {
        let mut rung = self.rung.lock().unwrap_or_else(|error| error.into_inner());
        *rung = true;
        self.woken.notify_one();
    }

    /// Waits for a ring, no longer than `limit`, and takes it.
    pub fn wait(&self, limit: Duration) {
        let rung = self.rung.lock().unwrap_or_else(|error| error.into_inner());
        let (mut rung, _) = self
            .woken
            .wait_timeout_while(rung, limit, |rung| !*rung)
            .unwrap_or_else(|error| error.into_inner());
        *rung = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn a_ring_before_the_wait_is_not_lost() {
        let wake = Wake::default();
        wake.ring();
        let started = Instant::now();

        wake.wait(Duration::from_secs(5));

        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_ring_from_another_thread_ends_the_wait() {
        let wake = Arc::new(Wake::default());
        let ringer = wake.clone();
        let started = Instant::now();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            ringer.ring();
        });

        wake.wait(Duration::from_secs(5));

        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_wait_with_no_ring_ends_at_its_limit() {
        let wake = Wake::default();
        let started = Instant::now();

        wake.wait(Duration::from_millis(20));

        assert!(started.elapsed() >= Duration::from_millis(20));
    }
}
