//! How fast a source is talking, in bytes a second.
//!
//! It is counted rather than worked out from the last reading: a window that
//! takes the bytes every 60 milliseconds takes a larger piece each time, and a
//! rate guessed from one piece and the wait before it would say the same thing
//! about a source that said nothing for half a second and a source that never
//! stopped. What is counted is what arrived inside [`OVER`], and the count is
//! the answer.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How far back the bytes are counted for the rate.
const OVER: Duration = Duration::from_secs(1);

/// What one kibibyte is, which the rate is answered in.
const KIBIBYTE: f32 = 1024.0;

/// Bytes counted over the last [`OVER`], answered as a rate.
#[derive(Debug, Default)]
pub struct RateMeter {
    arrivals: VecDeque<(Instant, usize)>,
    counted: usize,
}

impl RateMeter {
    /// A meter that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts bytes that have just arrived.
    pub fn push(&mut self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        self.arrivals.push_back((Instant::now(), bytes));
        self.counted += bytes;
    }

    /// Kibibytes a second over the last [`OVER`].
    ///
    /// It is asked of a meter that may have been given nothing for a while, so
    /// what fell out of the window is dropped here rather than when something
    /// arrives: a source that stopped talking reads as nought and not as what
    /// it last said.
    pub fn per_second(&mut self) -> f32 {
        self.forget_what_is_old();
        self.counted as f32 / OVER.as_secs_f32() / KIBIBYTE
    }

    /// Forgets everything counted so far, which is what a new connection is.
    pub fn clear(&mut self) {
        self.arrivals.clear();
        self.counted = 0;
    }

    /// Drops what arrived longer ago than the window.
    fn forget_what_is_old(&mut self) {
        let start = Instant::now() - OVER;
        while let Some((at, bytes)) = self.arrivals.front() {
            if *at >= start {
                break;
            }
            self.counted -= bytes;
            self.arrivals.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What arrived inside the window is what the rate says, and it is a rate
    /// of kibibytes rather than of bytes.
    #[test]
    fn the_rate_is_what_arrived_inside_the_window() {
        let mut meter = RateMeter::new();
        assert_eq!(meter.per_second(), 0.0, "nothing arrived yet");

        meter.push(2048);
        assert!((meter.per_second() - 2.0).abs() < 0.01);

        meter.push(1024);
        assert!((meter.per_second() - 3.0).abs() < 0.01);
    }

    /// A source that stopped talking reads as nought rather than as what it
    /// last said, because the window is emptied where the rate is asked for.
    #[test]
    fn a_source_that_stopped_reads_as_nothing() {
        let mut meter = RateMeter::new();
        meter.push(4096);
        assert!(meter.per_second() > 0.0);

        std::thread::sleep(OVER + Duration::from_millis(50));
        assert_eq!(meter.per_second(), 0.0);
    }

    /// Nothing counted is nothing added, so a reading that brought no bytes
    /// does not become an arrival of its own.
    #[test]
    fn nothing_is_not_an_arrival() {
        let mut meter = RateMeter::new();
        meter.push(0);
        assert_eq!(meter.per_second(), 0.0);

        meter.push(1024);
        meter.clear();
        assert_eq!(meter.per_second(), 0.0, "and a new connection starts over");
    }
}
