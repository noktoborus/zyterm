//! What a session says about a transfer while it is happening.

/// What a session tells the caller as it goes.
///
/// The library says what happened and in what order; how it reads, and whether
/// it is said at all, belongs to whoever is driving. Every method has a body
/// that does nothing, so a caller says only what it cares about.
pub trait Report {
    /// A command is about to be sent to the device, folded into its one line.
    fn command(&mut self, script: &str) {
        let _ = script;
    }

    /// A chunk of the file at hand is about to travel, `start` to `end`.
    fn chunk(&mut self, start: u64, end: u64) {
        let _ = (start, end);
    }

    /// The chunk that was travelling is over, and this is how much of it
    /// arrived; the next chunk starts where this one really ended.
    fn carried(&mut self, bytes: u64) {
        let _ = bytes;
    }
}

/// A caller that wants to hear nothing.
impl Report for () {}

/// A report shared with whoever set it, so the caller can still tell it which
/// file is travelling while the session tells it how far along that file is.
impl<T: Report> Report for std::rc::Rc<std::cell::RefCell<T>> {
    fn command(&mut self, script: &str) {
        self.borrow_mut().command(script);
    }

    fn chunk(&mut self, start: u64, end: u64) {
        self.borrow_mut().chunk(start, end);
    }

    fn carried(&mut self, bytes: u64) {
        self.borrow_mut().carried(bytes);
    }
}
