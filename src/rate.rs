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

/// How far back the bytes a script reports are counted for its speed.
///
/// Longer than [`OVER`]: a script says its bytes a few times a second at most,
/// and a rate over one second of such reports is a number that jumps.
const PACE_OVER: Duration = Duration::from_secs(3);

/// How fast the bytes a script reports go, from what it said and when.
///
/// It is told how many bytes of a file went so far, not how many arrived, so
/// the speed is the growth of that count over the time it took. The first
/// report of a file is where the counting starts and is no growth: a file
/// resumed at its middle says so at once, and that is not a megabyte carried
/// in no time. A count smaller than the last is another file, and the counting
/// starts again.
#[derive(Debug, Default)]
pub struct Pace {
    said: VecDeque<(Instant, u64)>,
}

impl Pace {
    /// Takes in that `done` bytes of the file went by now.
    pub fn push(&mut self, done: u64) {
        self.push_at(Instant::now(), done);
    }

    fn push_at(&mut self, at: Instant, done: u64) {
        if self.said.back().is_some_and(|(_, last)| done < *last) {
            self.said.clear();
        }
        self.said.push_back((at, done));
        // The last report older than the window is kept: it is where the
        // growth inside the window is counted from.
        while self.said.len() > 2 && self.said[1].0 + PACE_OVER < at {
            self.said.pop_front();
        }
    }

    /// Kibibytes a second over the last [`PACE_OVER`], or nothing before
    /// there are two reports to measure between.
    ///
    /// It is measured up to now and not up to the last report, so a transfer
    /// that stopped moving reads as slowing down and then as nought rather
    /// than as the speed it last had.
    pub fn per_second(&self) -> Option<f32> {
        self.per_second_at(Instant::now())
    }

    fn per_second_at(&self, now: Instant) -> Option<f32> {
        let (&(first_at, first), &(last_at, last)) = (self.said.front()?, self.said.back()?);
        if last_at == first_at {
            return None;
        }
        if last_at + PACE_OVER < now {
            return Some(0.0);
        }
        let span = now.saturating_duration_since(first_at).as_secs_f32();
        Some((last - first) as f32 / span / KIBIBYTE)
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

    /// The speed is the growth of the count over the time it took, counted
    /// from the first report of the file and not from nothing.
    #[test]
    fn a_pace_is_the_growth_of_the_count() {
        let start = Instant::now();
        let mut pace = Pace::default();
        pace.push_at(start, 1024 * 1024);
        assert_eq!(pace.per_second_at(start), None, "one report is no growth");

        pace.push_at(start + Duration::from_secs(1), 1024 * 1024 + 4096);
        let speed = pace
            .per_second_at(start + Duration::from_secs(1))
            .expect("two reports");
        assert!(
            (speed - 4.0).abs() < 0.01,
            "{speed}: a resumed file is not fast"
        );
    }

    /// A smaller count is another file, and the counting starts again.
    #[test]
    fn a_pace_starts_again_with_the_next_file() {
        let start = Instant::now();
        let mut pace = Pace::default();
        pace.push_at(start, 0);
        pace.push_at(start + Duration::from_secs(1), 8192);
        pace.push_at(start + Duration::from_secs(1), 0);
        assert_eq!(
            pace.per_second_at(start + Duration::from_secs(1)),
            None,
            "the new file has one report"
        );
    }

    /// A transfer that stopped moving slows down and then reads as nought.
    #[test]
    fn a_pace_that_stopped_reads_as_nothing() {
        let start = Instant::now();
        let mut pace = Pace::default();
        pace.push_at(start, 0);
        pace.push_at(start + Duration::from_secs(1), 4096);

        let later = pace
            .per_second_at(start + Duration::from_secs(2))
            .expect("two reports");
        assert!(
            (later - 2.0).abs() < 0.01,
            "{later}: halved, a second later"
        );
        assert_eq!(
            pace.per_second_at(start + PACE_OVER + Duration::from_secs(2)),
            Some(0.0)
        );
    }
}
