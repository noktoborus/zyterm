//! Buffer shared between the gateway threads and the caller.

use std::sync::{Arc, Condvar, Mutex};

/// Byte buffer filled by one side and swapped out by the other.
#[derive(Debug, Default)]
pub struct ByteSwap {
    filled: Mutex<Vec<u8>>,
    ready: Condvar,
}

impl ByteSwap {
    /// Empty buffer with the given reserved capacity.
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            filled: Mutex::new(Vec::with_capacity(capacity)),
            ready: Condvar::new(),
        })
    }

    /// Appends one chunk.
    pub fn push(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if let Ok(mut filled) = self.filled.lock() {
            filled.extend_from_slice(data);
            self.ready.notify_all();
        }
    }

    /// Exchanges the shared buffer with `spare`, which is cleared first.
    pub fn take_into(&self, spare: &mut Vec<u8>) {
        spare.clear();
        if let Ok(mut filled) = self.filled.lock()
            && !filled.is_empty()
        {
            std::mem::swap(&mut *filled, spare);
        }
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
    }

    /// Wakes a thread waiting in [`ByteSwap::take_into_wait`].
    pub fn wake(&self) {
        self.ready.notify_all();
    }

    /// True when nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.filled
            .lock()
            .map(|filled| filled.is_empty())
            .unwrap_or(true)
    }
}
