//! The buffer one thread fills and another swaps out.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Byte buffer filled by one side and swapped out by the other.
///
/// The buffer has a size, it is allocated at that size, and the side that
/// fills it reads no more than fits: it is held back instead
/// ([`ByteSwap::fill`]). A program or a device that says more than the window
/// takes would otherwise be a buffer growing for as long as it talks, and a
/// reader that stops reading is what a pipe or a driver answers by holding the
/// other side still.
///
/// What a read brought always goes in. A size lowered while a read is under
/// way is reached with the next read, not by throwing away bytes that are
/// already out of the driver or the pipe: the buffer may stand over the new
/// size by that one read, and the side that fills waits until the window took
/// it.
///
/// It follows the size downwards as well. A size lowered while the buffer stands
/// is memory the caller asked the window to stop keeping, and a buffer that only
/// ever grew would hold the largest size it was ever given until the source was
/// let go of.
///
/// A size of zero is a buffer with no limit, which is what the outgoing
/// direction stands at: what this program writes is what it decided to write.
///
/// Payload bytes never travel through a queue or a channel: the side that
/// fills appends whole chunks into the buffer, the side that takes swaps it
/// with a spare one of its own. One short lock and one pointer swap per
/// handover, and no allocation at all once both buffers stand at their size.
#[derive(Debug, Default)]
pub struct ByteSwap {
    filled: Mutex<Vec<u8>>,
    ready: Condvar,
    room: Condvar,
    size: AtomicUsize,
}

impl ByteSwap {
    /// Empty buffer of the given size, allocated now.
    pub fn with_size(bytes: usize) -> Arc<Self> {
        Arc::new(Self {
            filled: Mutex::new(Vec::with_capacity(bytes)),
            ready: Condvar::new(),
            room: Condvar::new(),
            size: AtomicUsize::new(bytes),
        })
    }

    /// Empty buffer with no limit and room for `capacity` bytes allocated now.
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            filled: Mutex::new(Vec::with_capacity(capacity)),
            ready: Condvar::new(),
            room: Condvar::new(),
            size: AtomicUsize::new(0),
        })
    }

    /// Gives the buffer another size, or zero for one with no limit.
    ///
    /// The buffer in hand is brought to the size at once — allocated up to it, or
    /// shrunk to it where it stood larger — and the one the caller is holding
    /// follows on the next swap, so both of them are at the size after one
    /// handover and neither is allocated again. Nothing but this and the two calls
    /// that take the bytes allocates either of them, and all of them happen
    /// because somebody asked for a size and not because bytes arrived.
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
    /// The side that fills the buffer reads this much and no more, so a chunk
    /// never carries it past its size and it is never grown to hold one.
    fn room(&self) -> usize {
        let size = self.size();
        if size == 0 {
            return usize::MAX;
        }
        size.saturating_sub(self.len())
    }

    /// Bytes the buffer holds room for without asking for more memory.
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

    /// Whether the buffer stands at its size.
    pub fn is_full(&self) -> bool {
        let size = self.size();
        size > 0 && self.len() >= size
    }

    /// Waits for room under the size, and answers whether there is any.
    ///
    /// It answers at once when there is no limit or the buffer is under it,
    /// and false when the wait ran out with the buffer still full, so the
    /// waiting thread can look at whether it is still wanted instead of waiting
    /// on a window that may never take another byte.
    fn wait_for_room(&self, timeout: Duration) -> bool {
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

    /// Appends a chunk, all of it.
    ///
    /// A side that keeps to a size fills through [`ByteSwap::fill`]; this is
    /// for a buffer with no limit, and for that call.
    pub fn push(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if let Ok(mut filled) = self.filled.lock() {
            filled.extend_from_slice(data);
        }
        self.ready.notify_all();
    }

    /// Reads into the buffer with `read`, as much as fits, and appends what it
    /// brought.
    ///
    /// It waits up to `wait` for room first, and answers `None` when there is
    /// still none: the side that fills looks at what else it has to do, and
    /// comes back. Otherwise `read` is handed the first bytes of `buffer`, as
    /// many as fit, and what it read goes in whole, however the size changed
    /// meanwhile. `Some(0)` is a read that brought nothing; what that means is
    /// the reader's.
    pub fn fill<E>(
        &self,
        buffer: &mut [u8],
        wait: Duration,
        read: impl FnOnce(&mut [u8]) -> Result<usize, E>,
    ) -> Result<Option<usize>, E> {
        if !self.wait_for_room(wait) {
            return Ok(None);
        }
        let room = self.room().min(buffer.len());
        if room == 0 {
            return Ok(None);
        }
        let count = read(&mut buffer[..room])?;
        self.push(&buffer[..count]);
        Ok(Some(count))
    }

    /// Exchanges the shared buffer with `spare`, which is cleared first.
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

    /// Waits until bytes are available or the timeout expires, then exchanges
    /// the shared buffer with `spare`, which is cleared first.
    pub fn take_into_wait(&self, spare: &mut Vec<u8>, timeout: Duration) {
        spare.clear();
        let Ok(mut filled) = self.filled.lock() else {
            return;
        };
        if filled.is_empty() {
            match self.ready.wait_timeout(filled, timeout) {
                Ok((guard, _)) => filled = guard,
                Err(_) => return,
            }
        }
        if !filled.is_empty() {
            std::mem::swap(&mut *filled, spare);
        }
        fit(&mut filled, self.size.load(Ordering::Relaxed));
        self.room.notify_all();
    }

    /// Wakes a thread waiting in [`ByteSwap::take_into_wait`] or for room in
    /// [`ByteSwap::fill`], which is how a side that is ending says so to
    /// whoever is waiting on it.
    pub fn wake(&self) {
        self.ready.notify_all();
        self.room.notify_all();
    }

    /// Bytes waiting for the consumer.
    pub fn len(&self) -> usize {
        self.filled.lock().map(|filled| filled.len()).unwrap_or(0)
    }

    /// True when nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops everything waiting.
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

    /// A read is handed as much as fits and no more, and a full buffer is not
    /// read into at all.
    #[test]
    fn a_read_is_handed_what_fits() {
        let swap = ByteSwap::with_size(4);
        let mut buffer = [0u8; 16];
        let read = swap.fill(&mut buffer, Duration::ZERO, |into| {
            assert_eq!(into.len(), 4, "the whole size");
            into[..2].copy_from_slice(b"ab");
            Ok::<_, ()>(2)
        });
        assert_eq!(read, Ok(Some(2)));
        let read = swap.fill(&mut buffer, Duration::ZERO, |into| {
            assert_eq!(into.len(), 2, "what is left");
            into.copy_from_slice(b"cd");
            Ok::<_, ()>(2)
        });
        assert_eq!(read, Ok(Some(2)));
        assert!(swap.is_full());
        let read = swap.fill(&mut buffer, Duration::ZERO, |_| -> Result<usize, ()> {
            panic!("a full buffer is not read into")
        });
        assert_eq!(read, Ok(None));

        let mut spare = Vec::new();
        swap.take_into(&mut spare);
        assert_eq!(spare, b"abcd");
    }

    /// A size lowered while a read is under way loses nothing: what the read
    /// brought goes in whole, and the next read waits until the window took it.
    #[test]
    fn a_size_lowered_during_a_read_loses_nothing() {
        let swap = ByteSwap::with_size(64);
        let mut buffer = [0u8; 64];
        let read = swap.fill(&mut buffer, Duration::ZERO, |into| {
            swap.set_size(16);
            into.fill(7);
            Ok::<_, ()>(into.len())
        });
        assert_eq!(read, Ok(Some(64)));
        assert_eq!(swap.len(), 64, "all of it went in");
        assert_eq!(
            swap.fill(&mut buffer, Duration::ZERO, |_| Ok::<_, ()>(0)),
            Ok(None),
            "and the next read waits for the window"
        );

        let mut spare = Vec::new();
        swap.take_into(&mut spare);
        assert_eq!(spare, [7u8; 64]);
        let read = swap.fill(&mut buffer, Duration::ZERO, |into| {
            assert_eq!(into.len(), 16, "the new size is kept from here on");
            Ok::<_, ()>(0)
        });
        assert_eq!(read, Ok(Some(0)));
    }

    /// What a read failed with comes back as it is, and nothing goes in.
    #[test]
    fn a_failed_read_comes_back_and_adds_nothing() {
        let swap = ByteSwap::with_size(8);
        let mut buffer = [0u8; 8];
        let read = swap.fill(&mut buffer, Duration::ZERO, |_| Err("gone"));
        assert_eq!(read, Err("gone"));
        assert!(swap.is_empty());
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
            swap.push(&[7u8; 64]);
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
        swap.push(&[0u8; 4096]);
        assert_eq!(swap.len(), 4096);
        assert!(!swap.is_full());
    }

    /// A side waiting for bytes is woken by the push that brings them.
    #[test]
    fn a_push_wakes_the_side_that_waits_for_bytes() {
        let swap = ByteSwap::with_capacity(16);
        let filler = {
            let swap = swap.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                swap.push(b"late");
            })
        };
        let mut spare = Vec::new();
        swap.take_into_wait(&mut spare, Duration::from_secs(5));
        filler.join().expect("the filler ends");
        assert_eq!(spare, b"late");
    }
}
