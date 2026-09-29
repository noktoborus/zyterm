//! The recent history of the signals of one port.
//!
//! A level says what a line is doing now, which is not what somebody watching a
//! line came to find out. A board pulled `DTR` and went quiet, `CTS` fell for
//! half a second and the data stopped with it, a device raised `DCD` and dropped
//! it again — all of that happens between two glances at a row of letters, and
//! the row looks the same afterwards as it did before. What is kept here is the
//! order those things happened in.
//!
//! One sample is one poll of the lines, taken whether the levels moved or not:
//! the picture drawn from it is a track over time, and a track carrying a sample
//! only where something changed has no time on its axis at all.
//!
//! A sample is a word of two bytes, one bit per signal: the five this side
//! drives and the four the peer does, counting each direction of the data as
//! one. A history of thousands of them is a few kibibytes.
//!
//! Nothing here knows what a track looks like. Which order the rows stand in,
//! what they are painted in and how wide a bar is are questions about a window,
//! and the answer to all of them is somewhere else.

use crate::lines::ControlLines;
use std::collections::VecDeque;

/// How many polls of the lines the history keeps.
///
/// Two bytes each, so the whole of it is sixteen kibibytes. A window draws one
/// sample to a pixel, so the depth has to clear the widest screen there is and
/// not the widest window somebody happens to have open: this is more than twice
/// the pixels across a four thousand pixel display, which is what keeps the
/// number from being one that has to be set against a screen at all.
pub const LINE_HISTORY_SAMPLES: usize = 8192;

/// One signal a sample carries, which is one bit of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Bytes went to the device.
    Sent,
    /// The break condition, held from this side.
    Break,
    /// The port is not being read from this side.
    Held,
    /// Request To Send.
    Rts,
    /// Data Terminal Ready.
    Dtr,
    /// Bytes came from the device.
    Received,
    /// Clear To Send.
    Cts,
    /// Data Set Ready.
    Dsr,
    /// Data Carrier Detect.
    Carrier,
}

impl Signal {
    /// Every signal a sample carries, which is every bit of it.
    pub const ALL: [Self; 9] = [
        Self::Sent,
        Self::Break,
        Self::Held,
        Self::Rts,
        Self::Dtr,
        Self::Received,
        Self::Cts,
        Self::Dsr,
        Self::Carrier,
    ];

    /// Whether this side drives the signal rather than the peer.
    ///
    /// It is a fact about the line and not about a window, so it is answered
    /// here: `RTS` is driven from this end on every machine there is, and a
    /// caller that had to know which of them are would be a caller keeping a
    /// second copy of the wiring.
    pub fn outgoing(self) -> bool {
        matches!(
            self,
            Self::Sent | Self::Break | Self::Held | Self::Rts | Self::Dtr
        )
    }

    /// Which bit of a sample stands for it.
    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// The signals of one poll of the lines, packed into one word.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineSample(u16);

impl LineSample {
    /// The sample of one poll: the lines as the driver reported them, what this
    /// side is holding — the break condition and the reading — and whether a
    /// byte crossed in either direction since the poll before it.
    ///
    /// The two lines this side drives are taken as [`ControlLines::rts_up`] and
    /// [`ControlLines::dtr_up`] answer them, so a platform that cannot read them
    /// back draws what was asked for rather than a line that is never up.
    pub fn new(
        lines: &ControlLines,
        held_break: bool,
        held: bool,
        sent: bool,
        received: bool,
    ) -> Self {
        Self::default()
            .with(Signal::Sent, sent)
            .with(Signal::Break, held_break)
            .with(Signal::Held, held)
            .with(Signal::Rts, lines.rts_up())
            .with(Signal::Dtr, lines.dtr_up())
            .with(Signal::Received, received)
            .with(Signal::Cts, lines.cts)
            .with(Signal::Dsr, lines.dsr)
            .with(Signal::Carrier, lines.cd)
    }

    /// Whether the given signal stood in this sample.
    pub fn has(self, signal: Signal) -> bool {
        self.0 & signal.bit() != 0
    }

    /// The same sample with one signal standing or not.
    fn with(self, signal: Signal, standing: bool) -> Self {
        match standing {
            true => Self(self.0 | signal.bit()),
            false => Self(self.0 & !signal.bit()),
        }
    }
}

/// The newest [`LINE_HISTORY_SAMPLES`] samples, oldest first.
#[derive(Debug)]
pub struct LineHistory {
    samples: VecDeque<LineSample>,
}

impl Default for LineHistory {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(LINE_HISTORY_SAMPLES),
        }
    }
}

impl LineHistory {
    /// A history that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a sample, dropping the oldest one when the history is full.
    pub fn push(&mut self, sample: LineSample) {
        if self.samples.len() >= LINE_HISTORY_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Copies at most `count` of the newest samples into `out`, oldest first.
    ///
    /// `out` is cleared and never grown past what it already stands at, so a
    /// caller handing the same buffer over every frame allocates once and never
    /// again.
    pub fn newest_into(&self, count: usize, out: &mut Vec<LineSample>) {
        out.clear();
        let older = self.samples.len().saturating_sub(count);
        out.extend(self.samples.iter().skip(older).copied());
    }

    /// Throws everything away, which is what a connection that is not the one
    /// before it begins with.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// How many samples it holds.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every signal reads back out of the byte it went into, and none of them
    /// answers for its neighbour.
    ///
    /// One signal per bit is one slip away from a window drawing the carrier
    /// where the break was, so each of them is set on its own and all the others
    /// are asked whether they moved.
    #[test]
    fn every_signal_of_a_sample_answers_only_for_itself() {
        for signal in Signal::ALL {
            let sample = LineSample::default().with(signal, true);

            assert!(sample.has(signal), "{signal:?}");
            for other in Signal::ALL.iter().filter(|other| **other != signal) {
                assert!(!sample.has(*other), "{signal:?} answered for {other:?}");
            }
        }
    }

    /// A sample is built from what the driver said, and the two lines this side
    /// drives are taken as they are reported rather than as they were asked
    /// for.
    #[test]
    fn a_sample_carries_what_the_driver_reported() {
        let lines = ControlLines {
            rts_asked: true,
            rts: Some(false),
            dtr_asked: false,
            dtr: Some(true),
            cts: true,
            dsr: false,
            cd: true,
            ri: false,
        };

        let sample = LineSample::new(&lines, true, false, false, true);

        assert!(!sample.has(Signal::Rts), "the line is what it is");
        assert!(sample.has(Signal::Dtr));
        assert!(sample.has(Signal::Cts));
        assert!(!sample.has(Signal::Dsr));
        assert!(sample.has(Signal::Carrier));
        assert!(sample.has(Signal::Break));
        assert!(!sample.has(Signal::Sent));
        assert!(sample.has(Signal::Received));
    }

    /// A line the platform cannot read back falls to what was asked for, so a
    /// window on Windows draws the level this side drove rather than a line
    /// that is never up.
    #[test]
    fn a_line_the_driver_does_not_report_is_what_was_asked_for() {
        let lines = ControlLines {
            rts_asked: true,
            dtr_asked: false,
            rts: None,
            dtr: None,
            ..ControlLines::default()
        };

        let sample = LineSample::new(&lines, false, false, false, false);

        assert!(sample.has(Signal::Rts));
        assert!(!sample.has(Signal::Dtr));
    }

    /// The signals this side drives are told from the ones the peer drives,
    /// which is what the two groups of a window are built from.
    #[test]
    fn the_signals_this_side_drives_are_named() {
        let outgoing: Vec<Signal> = Signal::ALL
            .into_iter()
            .filter(|signal| signal.outgoing())
            .collect();

        assert_eq!(
            outgoing,
            vec![
                Signal::Sent,
                Signal::Break,
                Signal::Held,
                Signal::Rts,
                Signal::Dtr
            ]
        );
    }

    /// The history keeps the newest and drops the oldest, in that order.
    ///
    /// A window reading it the other way round would draw the past on the right
    /// and call it now.
    #[test]
    fn the_oldest_sample_is_the_one_that_goes_when_the_history_is_full() {
        let older = LineSample::default().with(Signal::Sent, true);
        let newer = LineSample::default();
        let pushed_out = 3;

        let mut history = LineHistory::new();
        for _ in 0..LINE_HISTORY_SAMPLES {
            history.push(older);
        }
        for _ in 0..pushed_out {
            history.push(newer);
        }

        assert_eq!(history.len(), LINE_HISTORY_SAMPLES, "it holds no more");

        let mut out = Vec::new();
        history.newest_into(LINE_HISTORY_SAMPLES, &mut out);

        assert_eq!(
            out.iter().filter(|sample| sample.has(Signal::Sent)).count(),
            LINE_HISTORY_SAMPLES - pushed_out,
            "the oldest three went and no others"
        );
        assert!(
            out[out.len() - pushed_out..]
                .iter()
                .all(|sample| !sample.has(Signal::Sent)),
            "and the newest stand last"
        );
        assert!(
            out[0].has(Signal::Sent),
            "the oldest that is left stands first"
        );
    }

    /// A copy into a buffer that already held something carries nothing of it,
    /// and asks for no more than it was told to: the window hands the same
    /// buffer over in every frame.
    #[test]
    fn a_copy_of_the_newest_samples_leaves_nothing_of_what_stood_there() {
        let mut history = LineHistory::new();
        for signal in [Signal::Rts, Signal::Cts, Signal::Dsr] {
            history.push(LineSample::default().with(signal, true));
        }

        let mut out = vec![LineSample::default().with(Signal::Break, true); 9];
        history.newest_into(2, &mut out);

        assert_eq!(out.len(), 2, "no more than was asked for");
        assert!(out[0].has(Signal::Cts), "and they are the newest two");
        assert!(out[1].has(Signal::Dsr));
        assert!(
            !out.iter().any(|sample| sample.has(Signal::Break)),
            "nothing of what stood in the buffer is left"
        );

        history.newest_into(99, &mut out);
        assert_eq!(
            out.len(),
            3,
            "asking for more than there is answers all of it"
        );

        history.clear();
        history.newest_into(99, &mut out);
        assert!(out.is_empty());
        assert!(history.is_empty());
    }
}
