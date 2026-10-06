//! The line a script writes to, and the gate that can shut it.
//!
//! The script runs on a thread of its own and must not reach the port: it
//! pushes into one buffer and takes from another, and the application moves
//! both once a frame. That is the same arrangement a transfer program had,
//! with the pipes of a process replaced by two swapped buffers.
//!
//! The gate is what makes a run cancellable for certain. A script that cannot
//! be stopped — stuck in a call this crate did not write, or past every
//! deadline — is left where it is and its channel is closed, so whatever it
//! does next reaches nothing. A byte written by a run that was given up on
//! would otherwise arrive in a session that has moved on.

use crate::chunks::ByteSwap;
use crate::host::Line;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// How much each direction keeps room for without allocating again.
const CAPACITY: usize = 64 * 1024;

/// The two directions of the line, with the gate that shuts them.
#[derive(Debug)]
pub struct LineChannel {
    from_line: Arc<ByteSwap>,
    to_line: Arc<ByteSwap>,
    pending: AtomicUsize,
    open: AtomicBool,
}

impl LineChannel {
    /// A channel with both directions empty and the gate open.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            from_line: ByteSwap::with_capacity(CAPACITY),
            to_line: ByteSwap::with_capacity(CAPACITY),
            pending: AtomicUsize::new(0),
            open: AtomicBool::new(true),
        })
    }

    /// Hands the script what the device said.
    pub fn feed(&self, data: &[u8]) {
        if self.is_open() {
            self.from_line.push(data);
        }
    }

    /// Takes what the script wrote for the device into `spare`, cleared first.
    pub fn take_output(&self, spare: &mut Vec<u8>) {
        self.to_line.take_into(spare);
    }

    /// True while the script has written something nobody has taken.
    pub fn has_output(&self) -> bool {
        !self.to_line.is_empty()
    }

    /// Says how much the application and the driver still hold for the device.
    pub fn set_pending_output(&self, bytes: usize) {
        self.pending.store(bytes, Ordering::Relaxed);
    }

    /// Shuts the gate: what the script does from here reaches nothing.
    pub fn close(&self) {
        self.open.store(false, Ordering::Relaxed);
        self.from_line.wake();
        self.to_line.wake();
    }
}

impl Line for LineChannel {
    fn write(&self, bytes: &[u8]) {
        if self.is_open() {
            self.to_line.push(bytes);
        }
    }

    fn read(&self, into: &mut Vec<u8>, timeout: Duration) {
        if !self.is_open() {
            into.clear();
            return;
        }
        self.from_line.take_into_wait(into, timeout);
    }

    fn pending_output(&self) -> usize {
        self.pending.load(Ordering::Relaxed) + usize::from(self.has_output())
    }

    fn waiting(&self) -> usize {
        self.from_line.waiting()
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    fn close(&self) {
        LineChannel::close(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_device_said_reaches_the_script_whole() {
        let channel = LineChannel::new();
        channel.feed(b"### 100");
        channel.feed(b" 4096\r\n");

        let mut read = Vec::new();
        channel.read(&mut read, Duration::from_millis(10));
        assert_eq!(read, b"### 100 4096\r\n");
    }

    #[test]
    fn a_read_with_nothing_waiting_answers_with_nothing() {
        let channel = LineChannel::new();
        let mut read = vec![b'x'];
        channel.read(&mut read, Duration::from_millis(5));
        assert!(read.is_empty(), "the buffer of the caller is cleared");
    }

    #[test]
    fn what_the_script_wrote_is_taken_once() {
        let channel = LineChannel::new();
        channel.write(b"\\pwd\n");
        assert!(channel.has_output());

        let mut out = Vec::new();
        channel.take_output(&mut out);
        assert_eq!(out, b"\\pwd\n");
        assert!(!channel.has_output());
    }

    #[test]
    fn what_the_device_said_can_be_looked_at_without_taking_it() {
        let channel = LineChannel::new();
        assert_eq!(channel.waiting(), 0);

        channel.feed(b"### 100\r\n");
        assert_eq!(channel.waiting(), 9);

        let mut read = Vec::new();
        channel.read(&mut read, Duration::from_millis(5));
        assert_eq!(read, b"### 100\r\n", "the look took nothing");
        assert_eq!(channel.waiting(), 0);
    }

    #[test]
    fn a_closed_gate_carries_nothing_in_either_direction() {
        let channel = LineChannel::new();
        channel.close();

        channel.write(b"too late");
        channel.feed(b"too late");

        let mut out = Vec::new();
        channel.take_output(&mut out);
        assert!(out.is_empty(), "nothing the script wrote reaches the line");

        let mut read = Vec::new();
        channel.read(&mut read, Duration::from_millis(5));
        assert!(read.is_empty(), "nothing of the line reaches the script");
        assert!(!channel.is_open());
    }

    #[test]
    fn what_waits_for_the_device_counts_both_sides() {
        let channel = LineChannel::new();
        channel.set_pending_output(12);
        assert_eq!(channel.pending_output(), 12);

        channel.write(b"x");
        assert_eq!(
            channel.pending_output(),
            13,
            "what the application has not taken yet counts too"
        );
    }
}
