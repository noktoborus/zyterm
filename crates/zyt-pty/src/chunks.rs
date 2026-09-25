//! Buffer shared between the pty threads and the caller.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Byte buffer filled by one side and swapped out by the other.
///
/// The buffer has a size, it is allocated at that size, and it never grows past
/// it: the side that fills it is held back instead. A program that says more
/// than the window takes would otherwise be a buffer growing for as long as it
/// talks, and a reader that stops reading is what a pty answers by holding the
/// program still — the pipe fills, and the next `write` of the program waits.
///
/// A size of zero is a buffer with no limit, which is what the outgoing
/// direction stands at: what this program writes is what it decided to write.
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

    /// Gives the buffer another size, or zero for one with no limit.
    ///
    /// This is the one place the buffer is allocated after it was made, and it
    /// happens when somebody asks for another size and not when bytes arrive.
    pub fn set_size(&self, bytes: usize) {
        self.size.store(bytes, Ordering::Relaxed);
        if let Ok(mut filled) = self.filled.lock() {
            reserve(&mut filled, bytes);
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
    pub fn room(&self) -> usize {
        let size = self.size();
        if size == 0 {
            return usize::MAX;
        }
        size.saturating_sub(self.len())
    }

    /// Bytes the buffer holds room for without asking for more memory.
    ///
    /// It is here for the test that the buffer is allocated once and never
    /// again; nothing outside this crate has a reason to ask.
    #[cfg(test)]
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
    pub fn wait_for_room(&self, timeout: std::time::Duration) -> bool {
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
    /// A caller that asked [`ByteSwap::room`] first is answered with all of it;
    /// one that did not keeps what is left rather than growing the buffer.
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
        self.ready.notify_all();
        taken
    }

    /// Exchanges the shared buffer with `spare`, which is cleared first.
    ///
    /// The buffer that comes back from the caller is brought up to the size
    /// before it is filled again, so a caller that started with an empty one
    /// costs one allocation and never another.
    pub fn take_into(&self, spare: &mut Vec<u8>) {
        spare.clear();
        if let Ok(mut filled) = self.filled.lock() {
            if !filled.is_empty() {
                std::mem::swap(&mut *filled, spare);
            }
            reserve(&mut filled, self.size.load(Ordering::Relaxed));
        }
        self.room.notify_all();
    }

    /// Waits until bytes are available or the timeout expires, then exchanges
    /// the shared buffer with `spare`, which is cleared first.
    pub fn take_into_wait(&self, spare: &mut Vec<u8>, timeout: std::time::Duration) {
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
        reserve(&mut filled, self.size.load(Ordering::Relaxed));
        self.room.notify_all();
    }

    /// Wakes a thread waiting in [`ByteSwap::take_into_wait`] or in
    /// [`ByteSwap::wait_for_room`], which is how a side that is ending says so
    /// to whoever is waiting on it.
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
}

/// Brings an empty buffer up to the size it is to hold, and leaves one that is
/// already there alone.
fn reserve(buffer: &mut Vec<u8>, size: usize) {
    if size > buffer.capacity() {
        buffer.reserve_exact(size - buffer.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buffer takes what fits and no more, and says how much that was, so
    /// the side that fills it keeps the rest instead of the buffer growing.
    #[test]
    fn a_buffer_takes_what_fits_and_says_how_much() {
        let swap = ByteSwap::with_size(4);
        assert_eq!(swap.room(), 4);

        assert_eq!(swap.push(b"ab"), 2);
        assert_eq!(swap.push(b"cdef"), 2, "two of the four fit");
        assert!(swap.is_full());
        assert_eq!(swap.push(b"g"), 0, "and nothing fits into a full one");

        let mut spare = Vec::new();
        swap.take_into(&mut spare);
        assert_eq!(spare, b"abcd");
    }

    /// Neither buffer is allocated again once both stand at the size: the bytes
    /// of a steady stream are appended into room that is already there, and the
    /// swap hands back a buffer that is already at the size.
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
