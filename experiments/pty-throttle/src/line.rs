//! One direction of a line: bytes wait in a queue and leave it at the speed of
//! the line.
//!
//! A byte arrives on the other end when its last stop bit has gone, so the
//! first byte written to an idle line arrives one byte time later, and the
//! bytes behind it one byte time apart. Time is counted from the moment the
//! line last stood idle, in whole nanoseconds of the bits sent since, so a long
//! run does not drift by the rounding of one byte time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::frame::Frame;

const NANOS: u128 = 1_000_000_000;

/// One direction of the line.
pub struct Line {
    queue: VecDeque<u8>,
    capacity: usize,
    baud: u128,
    bits: u128,
    mask: u8,
    epoch: Instant,
    sent: u64,
    total: u64,
}

impl Line {
    /// An idle line of `baud` bits a second, holding up to `capacity` bytes
    /// that wait to go.
    pub fn new(baud: u32, frame: Frame, capacity: usize, now: Instant) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            baud: u128::from(baud),
            bits: u128::from(frame.bits()),
            mask: frame.mask(),
            epoch: now,
            sent: 0,
            total: 0,
        }
    }

    /// How many bytes more the queue takes.
    pub fn room(&self) -> usize {
        self.capacity - self.queue.len()
    }

    /// True when nothing waits to go.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Bytes that went, since the start.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Queues bytes; the bits a frame does not carry are lost, as on the wire.
    /// An idle line starts its count again, so time it stood idle is not
    /// credited to the bytes after it.
    pub fn push(&mut self, bytes: &[u8], now: Instant) {
        if self.queue.is_empty() {
            self.epoch = now;
            self.sent = 0;
        }
        let mask = self.mask;
        self.queue.extend(bytes.iter().map(|byte| byte & mask));
    }

    /// How many waiting bytes have arrived by `now`.
    pub fn ready(&self, now: Instant) -> usize {
        let elapsed = now.saturating_duration_since(self.epoch).as_nanos();
        let arrived = elapsed * self.baud / (self.bits * NANOS);
        let due = arrived.saturating_sub(u128::from(self.sent));
        usize::try_from(due).map_or(self.queue.len(), |due| due.min(self.queue.len()))
    }

    /// When the next waiting byte arrives.
    pub fn next(&self) -> Option<Instant> {
        if self.queue.is_empty() {
            return None;
        }
        let bits = (u128::from(self.sent) + 1) * self.bits * NANOS;
        let nanos = bits.div_ceil(self.baud);
        let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
        Some(self.epoch + Duration::from_nanos(nanos))
    }

    /// The first `count` waiting bytes, in one slice.
    pub fn front(&mut self, count: usize) -> &[u8] {
        &self.queue.make_contiguous()[..count]
    }

    /// Takes `count` bytes off the queue as gone.
    pub fn consume(&mut self, count: usize) {
        self.queue.drain(..count);
        self.sent += count as u64;
        self.total += count as u64;
    }

    /// Throws away everything that waits.
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(baud: u32, frame: &str, start: Instant) -> Line {
        Line::new(baud, frame.parse().unwrap(), 4096, start)
    }

    #[test]
    fn bytes_arrive_one_byte_time_apart() {
        let start = Instant::now();
        let mut line = line(9600, "8N1", start);
        line.push(&[0; 100], start);
        let byte = Duration::from_nanos(1_041_667);
        assert_eq!(line.ready(start), 0);
        assert_eq!(line.next(), Some(start + byte));
        assert_eq!(line.ready(start + byte), 1);
        assert_eq!(line.ready(start + Duration::from_millis(50)), 48);
        assert_eq!(line.ready(start + Duration::from_secs(1)), 100);
        line.consume(48);
        assert_eq!(line.ready(start + Duration::from_millis(50)), 0);
        assert_eq!(line.ready(start + Duration::from_millis(100)), 48);
    }

    #[test]
    fn a_second_carries_the_bytes_of_the_speed() {
        let start = Instant::now();
        for (baud, frame, bytes) in [
            (9600, "8N1", 960),
            (57600, "8N1", 5760),
            (115200, "8N1", 11520),
            (9600, "7E2", 872),
        ] {
            let mut line = line(baud, frame, start);
            line.push(&[0; 4096], start);
            assert_eq!(line.ready(start + Duration::from_secs(1)), bytes.min(4096));
        }
    }

    #[test]
    fn an_idle_line_credits_nothing() {
        let start = Instant::now();
        let mut line = line(9600, "8N1", start);
        line.push(b"a", start);
        line.consume(line.ready(start + Duration::from_secs(1)));
        let later = start + Duration::from_secs(10);
        line.push(b"bcd", later);
        assert_eq!(line.ready(later), 0);
        assert_eq!(line.ready(later + Duration::from_millis(2)), 1);
    }

    #[test]
    fn a_frame_of_seven_bits_loses_the_eighth() {
        let start = Instant::now();
        let mut line = line(9600, "7N1", start);
        line.push(&[0xd1, 0x9c], start);
        assert_eq!(line.front(2), [0x51, 0x1c]);
    }

    #[test]
    fn the_queue_holds_its_capacity() {
        let start = Instant::now();
        let mut line = Line::new(9600, Frame::default(), 10, start);
        line.push(&[0; 4], start);
        assert_eq!(line.room(), 6);
        line.clear();
        assert!(line.is_empty());
        assert_eq!(line.next(), None);
    }
}
