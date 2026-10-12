//! Holding a new size of the grid back until the area stops changing.
//!
//! An area dragged wider or narrower changes on every frame, and a grid
//! resized on every one of them is reflowed as often and tells the program as
//! often. A shell redraws its prompt each time, from where it believes the
//! prompt stood, and a device on a line redraws whatever it draws over a page
//! that has moved under it; both leave what they drew before behind. Held back,
//! the grid keeps the size the program was last told, takes its output at that
//! size, and changes once, when the area has stood still for the delay.

/// What the widget does about the size this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Step {
    /// The grid already has the size of the area.
    Settled,
    /// Resize the grid now.
    Now,
    /// Keep the size the grid has, and look again after this many seconds.
    Wait(f64),
}

/// The size the area asks for and since when it has asked for it.
#[derive(Debug, Default)]
pub(crate) struct Deferred {
    pending: Option<((usize, usize), f64)>,
    settled_once: bool,
}

impl Deferred {
    /// Decides whether a grid of `current` cells takes the `wanted` size of the
    /// area at `now`, in seconds, after the area has stood still for `delay`.
    ///
    /// The first size is taken at once: a grid that has never had the size of
    /// its area is not one a program has drawn on at the wrong size, and a
    /// window opened at the size of the configuration would otherwise show its
    /// first lines in a corner of itself.
    pub(crate) fn step(
        &mut self,
        current: (usize, usize),
        wanted: (usize, usize),
        now: f64,
        delay: f64,
    ) -> Step {
        if current == wanted {
            self.pending = None;
            self.settled_once = true;
            return Step::Settled;
        }
        if !self.settled_once || delay <= 0.0 {
            self.pending = None;
            self.settled_once = true;
            return Step::Now;
        }

        let since = match self.pending {
            Some((size, since)) if size == wanted => since,
            _ => now,
        };
        let waited = now - since;
        if waited >= delay {
            self.pending = None;
            return Step::Now;
        }
        self.pending = Some((wanted, since));
        Step::Wait(delay - waited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first size of the area is taken at once, whatever the delay.
    #[test]
    fn the_first_size_is_taken_at_once() {
        let mut deferred = Deferred::default();
        assert_eq!(deferred.step((80, 24), (100, 30), 0.0, 0.1), Step::Now);
    }

    /// A size that keeps changing is held back, and taken once it has stood
    /// still for the delay.
    #[test]
    fn a_size_is_taken_once_it_stands_still() {
        let mut deferred = Deferred::default();
        assert_eq!(deferred.step((80, 24), (80, 24), 0.0, 0.1), Step::Settled);

        assert_eq!(
            deferred.step((80, 24), (79, 24), 1.00, 0.1),
            Step::Wait(0.1)
        );
        assert!(matches!(
            deferred.step((80, 24), (78, 24), 1.05, 0.1),
            Step::Wait(_)
        ));
        assert!(
            matches!(deferred.step((80, 24), (78, 24), 1.10, 0.1), Step::Wait(_)),
            "the clock starts again at every change"
        );
        assert_eq!(deferred.step((80, 24), (78, 24), 1.16, 0.1), Step::Now);
    }

    /// An area that comes back to the size of the grid has nothing to do.
    #[test]
    fn a_size_that_comes_back_is_nothing_to_do() {
        let mut deferred = Deferred::default();
        deferred.step((80, 24), (80, 24), 0.0, 0.1);
        deferred.step((80, 24), (70, 24), 1.0, 0.1);

        assert_eq!(deferred.step((80, 24), (80, 24), 1.05, 0.1), Step::Settled);
        assert!(
            matches!(deferred.step((80, 24), (70, 24), 1.06, 0.1), Step::Wait(_)),
            "and a later change waits from where it began"
        );
    }

    /// No delay is no holding back.
    #[test]
    fn no_delay_takes_every_size() {
        let mut deferred = Deferred::default();
        deferred.step((80, 24), (80, 24), 0.0, 0.0);
        assert_eq!(deferred.step((80, 24), (79, 24), 1.0, 0.0), Step::Now);
    }
}
