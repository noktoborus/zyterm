//! Byte transport between the port thread and the user interface thread.
//!
//! Payload bytes never travel through a queue or a channel: the producer
//! appends whole chunks into a shared buffer, the consumer swaps that buffer
//! with its own spare one. One short lock and one pointer swap per frame, and
//! no allocation at all once both buffers stand at their size.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Shared double buffer for one direction.
///
/// The buffer has a size, it is allocated at that size, and it never grows
/// past it: the producer is held back instead. A device that says more than
/// the window takes would otherwise be a buffer growing for as long as it talks,
/// and the memory of a window is not a place to put a device that never stops.
///
/// It follows the size downwards as well. A size lowered while the buffer stands
/// is memory the caller asked the window to stop keeping, and a buffer that only
/// ever grew would hold the largest size it was ever given until the source was
/// let go of.
///
/// A size of zero is a buffer with no limit, which is what the outgoing
/// direction stands at: what this program writes is what it decided to write.
#[derive(Debug, Default)]
pub struct ByteSwap {
    filled: Mutex<Vec<u8>>,
    room: Condvar,
    size: AtomicUsize,
}

impl ByteSwap {
    /// Empty buffer of the given size, allocated now.
    pub fn with_size(bytes: usize) -> Arc<Self> {
        Arc::new(Self {
            filled: Mutex::new(Vec::with_capacity(bytes)),
            room: Condvar::new(),
            size: AtomicUsize::new(bytes),
        })
    }

    /// Gives the buffer another size, or zero for one with no limit.
    ///
    /// The buffer in hand is brought to the size at once — allocated up to it, or
    /// shrunk to it where it stood larger — and the one the caller is holding
    /// follows on the next swap, so both of them are at the size after one
    /// handover and neither is allocated again. Nothing but this and
    /// [`ByteSwap::take_into`] allocates either of them, and both happen because
    /// somebody asked for a size and not because bytes arrived.
    ///
    /// The size is read where the producer waits, so one raised while a
    /// producer stands at the old one lets it on as soon as it next looks.
    pub fn set_size(&self, bytes: usize) {
        self.size.store(bytes, Ordering::Relaxed);
        if let Ok(mut filled) = self.filled.lock() {
            fit(&mut filled, bytes);
        }
        self.room.notify_all();
    }

    /// The size in force, or zero when there is no limit.
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Bytes that fit right now, which is everything when there is no limit.
    ///
    /// The producer reads this much and no more, so a chunk never carries the
    /// buffer past its size and the buffer is never grown to hold one.
    pub fn room(&self) -> usize {
        let size = self.size();
        if size == 0 {
            return usize::MAX;
        }
        size.saturating_sub(self.len())
    }

    /// Whether the buffer stands at its size, so nothing more should be read
    /// into it.
    pub fn is_full(&self) -> bool {
        let size = self.size();
        size > 0 && self.len() >= size
    }

    /// Waits for room under the size, and answers whether there is any.
    ///
    /// It answers at once when there is no limit or the buffer is under it,
    /// and false when the wait ran out with the buffer still full — which is
    /// what lets the producer look at whatever else it has to do, the commands
    /// it is given and the end of its own life among them, instead of waiting
    /// on a window that may never take another byte.
    pub fn wait_for_room(&self, timeout: Duration) -> bool {
        let size = self.size();
        if size == 0 {
            return true;
        }
        let Ok(filled) = self.filled.lock() else {
            return true;
        };
        if filled.len() < size {
            return true;
        }
        match self.room.wait_timeout(filled, timeout) {
            Ok((filled, _)) => filled.len() < size,
            Err(_) => true,
        }
    }

    /// Appends what fits and answers how much that was.
    ///
    /// A producer that asked [`ByteSwap::room`] first is answered with all of
    /// it; one that did not keeps what is left rather than growing the buffer
    /// with it.
    pub fn push(&self, data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        let Ok(mut filled) = self.filled.lock() else {
            return 0;
        };
        let size = self.size.load(Ordering::Relaxed);
        let taken = if size == 0 {
            data.len()
        } else {
            data.len().min(size.saturating_sub(filled.len()))
        };
        filled.extend_from_slice(&data[..taken]);
        taken
    }

    /// Exchanges the shared buffer with the caller owned `spare`, which is
    /// cleared first. After the call `spare` holds everything produced since
    /// the previous call.
    ///
    /// The buffer that comes back from the caller is brought to the size before it
    /// is filled again, so a caller that started with an empty one costs one
    /// allocation and never another — and so a size that was lowered reaches the
    /// second of the two buffers here, at the one moment it holds nothing.
    pub fn take_into(&self, spare: &mut Vec<u8>) {
        spare.clear();
        if let Ok(mut filled) = self.filled.lock() {
            if !filled.is_empty() {
                std::mem::swap(&mut *filled, spare);
            }
            fit(&mut filled, self.size.load(Ordering::Relaxed));
        }
        self.room.notify_all();
    }

    /// Bytes the shared buffer holds room for without asking for more memory.
    ///
    /// It stands at the size once the buffer has been filled and taken once, a
    /// steady stream never moves it again, and a size that was lowered brings it
    /// down rather than leaving the memory of the larger one behind.
    pub fn capacity(&self) -> usize {
        self.filled
            .lock()
            .map(|filled| filled.capacity())
            .unwrap_or(0)
    }

    /// Number of bytes waiting for the consumer.
    pub fn len(&self) -> usize {
        self.filled.lock().map(|filled| filled.len()).unwrap_or(0)
    }

    /// True when nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops everything currently buffered.
    pub fn clear(&self) {
        if let Ok(mut filled) = self.filled.lock() {
            filled.clear();
        }
        self.room.notify_all();
    }
}

/// Brings a buffer to the size it is to hold.
///
/// One that stands under the size is allocated up to it, and one that stands over
/// it gives the memory back: a size lowered in the settings is memory the window
/// was asked to stop keeping, and a buffer that only ever grew would keep the
/// largest size it was ever given for as long as the source stayed open.
///
/// Both buffers reach the size this way, because the two change places: the one
/// handed back by the caller is brought to the size before it is filled again, so
/// a size that was lowered lands on both of them within one swap and neither is
/// allocated again after that.
///
/// A size of nought is no limit at all, and a buffer under one is left exactly as
/// it is: there is no size to bring it to, and the memory it holds room for is
/// what it was handed.
fn fit(buffer: &mut Vec<u8>, size: usize) {
    if size == 0 {
        return;
    }
    if size > buffer.capacity() {
        buffer.reserve_exact(size - buffer.len());
    } else if buffer.capacity() > size {
        buffer.shrink_to(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A size lowered is memory given back and not only a limit lowered.
    ///
    /// The buffer of a port is made at the size the worker was configured with and
    /// the window says what it is to be a moment later, so a buffer that only ever
    /// grew would keep that first size for as long as the source stayed open —
    /// which is exactly the memory somebody lowering the setting asked it not to
    /// keep.
    #[test]
    fn a_size_lowered_gives_the_memory_back() {
        let swap = ByteSwap::with_size(256 * 1024);
        assert!(
            swap.capacity() >= 256 * 1024,
            "allocated at the size it was made"
        );

        swap.set_size(4 * 1024);

        assert_eq!(swap.size(), 4 * 1024);
        assert!(
            swap.capacity() >= 4 * 1024,
            "and still holds the size it is at"
        );
        assert!(
            swap.capacity() < 256 * 1024,
            "the memory of the larger size is given back"
        );
    }

    /// Both buffers follow a lowered size, because the two change places: the one
    /// the caller is holding is brought to the size the moment it comes back, and
    /// it comes back holding nothing.
    ///
    /// Without that, the larger of the two would be handed to the producer on the
    /// next swap and the memory would stand for as long as the source did, walking
    /// from one side to the other.
    #[test]
    fn the_buffer_the_caller_hands_back_follows_the_size_as_well() {
        let swap = ByteSwap::with_size(256 * 1024);
        let mut spare = Vec::new();
        swap.push(&[7u8; 1024]);
        swap.take_into(&mut spare);
        assert!(
            spare.capacity() >= 256 * 1024,
            "the caller is holding the large one now"
        );

        swap.set_size(4 * 1024);
        swap.push(&[7u8; 1024]);
        swap.take_into(&mut spare);

        assert!(swap.capacity() < 256 * 1024, "the shared buffer came down");
        assert!(
            spare.capacity() < 256 * 1024,
            "and so did the one that went back in"
        );
        assert_eq!(spare, &[7u8; 1024], "with the bytes still in it");
    }

    /// A buffer with no limit is left exactly as it is: there is no size to bring
    /// it to, and the outgoing direction stands at one.
    #[test]
    fn a_buffer_with_no_limit_keeps_the_room_it_has() {
        let swap = ByteSwap::with_size(0);
        let mut spare = Vec::new();
        swap.push(&[7u8; 4096]);
        swap.take_into(&mut spare);
        swap.push(&[7u8; 4096]);
        let grown = swap.capacity();
        assert!(grown >= 4096);

        swap.take_into(&mut spare);
        swap.set_size(0);

        assert_eq!(
            swap.capacity(),
            grown,
            "nothing was given back or asked for"
        );
    }

    #[test]
    fn swap_moves_all_bytes_and_reuses_buffers() {
        let swap = ByteSwap::with_size(16);
        let mut spare = Vec::with_capacity(16);

        swap.push(b"abc");
        swap.push(b"def");
        swap.take_into(&mut spare);
        assert_eq!(spare, b"abcdef");
        assert!(swap.is_empty());

        swap.push(b"xy");
        swap.take_into(&mut spare);
        assert_eq!(spare, b"xy");
    }

    /// A buffer at its size holds the producer back until the consumer has
    /// taken what is in it, and a buffer with no limit never holds anybody
    /// back.
    #[test]
    fn a_full_buffer_holds_the_producer_back_until_it_is_taken() {
        let swap = ByteSwap::with_size(0);
        assert!(swap.wait_for_room(Duration::ZERO), "no limit, no wait");

        swap.set_size(4);
        swap.push(b"abcd");
        assert!(swap.is_full());
        assert!(
            !swap.wait_for_room(Duration::from_millis(10)),
            "the wait runs out while the buffer stands full"
        );

        let mut spare = Vec::new();
        swap.take_into(&mut spare);
        assert_eq!(spare, b"abcd");
        assert!(
            swap.wait_for_room(Duration::ZERO),
            "and there is room again"
        );
        assert!(!swap.is_full());
    }

    /// The producer waiting on a full buffer is let on the moment the consumer
    /// takes what is in it, and not when its wait runs out.
    #[test]
    fn taking_the_bytes_lets_the_producer_on_at_once() {
        let swap = ByteSwap::with_size(4);
        swap.push(b"abcd");

        let taker = {
            let swap = swap.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                let mut spare = Vec::new();
                swap.take_into(&mut spare);
            })
        };

        let started = std::time::Instant::now();
        assert!(swap.wait_for_room(Duration::from_secs(5)));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the wait ended with the take and not with the timeout"
        );
        taker.join().expect("the taker ends");
    }

    /// A size raised over what is waiting lets the producer on: it is read
    /// where the producer waits, so a window that was given a larger one says
    /// so at once.
    #[test]
    fn a_size_raised_over_what_is_waiting_lets_the_producer_on() {
        let swap = ByteSwap::with_size(4);
        swap.push(b"abcd");
        assert!(!swap.wait_for_room(Duration::from_millis(10)));

        swap.set_size(8);
        assert!(swap.wait_for_room(Duration::ZERO));
        swap.set_size(0);
        assert!(
            swap.wait_for_room(Duration::ZERO),
            "and no limit at all lets it on"
        );
    }

    #[test]
    fn take_into_clears_when_nothing_was_produced() {
        let swap = ByteSwap::with_size(4);
        let mut spare = vec![1u8, 2, 3];
        swap.take_into(&mut spare);
        assert!(spare.is_empty());
    }

    /// The buffer takes what fits and no more, and says how much that was, so
    /// the producer keeps the rest instead of the buffer growing to hold it.
    #[test]
    fn a_buffer_takes_what_fits_and_says_how_much() {
        let swap = ByteSwap::with_size(4);
        assert_eq!(swap.room(), 4);

        assert_eq!(swap.push(b"ab"), 2);
        assert_eq!(swap.room(), 2);
        assert_eq!(swap.push(b"cdef"), 2, "two of the four fit");
        assert!(swap.is_full());
        assert_eq!(swap.room(), 0);
        assert_eq!(swap.push(b"g"), 0, "and nothing fits into a full one");

        let mut spare = Vec::new();
        swap.take_into(&mut spare);
        assert_eq!(spare, b"abcd");
    }

    /// Neither buffer is allocated again once both stand at the size: the
    /// bytes of a steady stream are appended into room that is already there,
    /// and the swap hands back a buffer that is already at the size.
    #[test]
    fn a_steady_stream_allocates_nothing() {
        let swap = ByteSwap::with_size(64);
        let mut spare = Vec::new();

        swap.push(&[7u8; 64]);
        swap.take_into(&mut spare);
        let (theirs, mine) = (swap.capacity(), spare.capacity());
        assert!(theirs >= 64 && mine >= 64, "both stand at the size");

        for _ in 0..16 {
            assert_eq!(swap.push(&[7u8; 64]), 64);
            swap.take_into(&mut spare);
        }
        assert_eq!(swap.capacity(), theirs, "the shared buffer never grew");
        assert_eq!(spare.capacity(), mine, "and neither did the spare");
    }

    /// A buffer with no limit takes everything, which is what the outgoing
    /// direction needs: what this program writes is not something to hold back.
    #[test]
    fn a_buffer_with_no_limit_takes_everything() {
        let swap = ByteSwap::with_size(0);
        assert_eq!(swap.room(), usize::MAX);
        assert_eq!(swap.push(&[0u8; 4096]), 4096);
        assert!(!swap.is_full());
    }
}
