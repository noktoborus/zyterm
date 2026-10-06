//! The line a script is given when there is no window.
//!
//! One shape covers every case: something to read the device from and
//! something to write it to. The standard channels of the process are that
//! pair, and so are the pipes of a program, and so are the two halves of a
//! pseudo terminal. A reader thread fills a buffer, exactly as the port worker
//! of the application does, so the script waits the same way it would there.

use crate::chunks::ByteSwap;
use crate::host::Line;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How much is read from the far side at once.
const READ: usize = 8 * 1024;

/// A line made of something to read and something to write.
pub struct Piped {
    from: Arc<ByteSwap>,
    to: Mutex<Box<dyn Write + Send>>,
    open: Arc<AtomicBool>,
}

impl Piped {
    /// A line over that pair, with a thread reading the one of them.
    pub fn new(
        mut reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
    ) -> crate::Result<Arc<Self>> {
        let from = ByteSwap::with_capacity(READ);
        let open = Arc::new(AtomicBool::new(true));

        crate::process::spawn_thread("zyt-script-line", {
            let from = from.clone();
            let open = open.clone();
            move || {
                let mut buffer = vec![0u8; READ];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => from.push(&buffer[..read]),
                    }
                }
                open.store(false, Ordering::Release);
                from.wake();
            }
        })?;

        Ok(Arc::new(Self {
            from,
            to: Mutex::new(Box::new(writer)),
            open,
        }))
    }
}

impl Line for Piped {
    fn write(&self, bytes: &[u8]) {
        if let Ok(mut to) = self.to.lock() {
            let _ = to.write_all(bytes);
            let _ = to.flush();
        }
    }

    fn read(&self, into: &mut Vec<u8>, timeout: Duration) {
        self.from.take_into_wait(into, timeout);
    }

    fn pending_output(&self) -> usize {
        0
    }

    fn waiting(&self) -> usize {
        self.from.waiting()
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire) || !self.from.is_empty()
    }

    fn close(&self) {
        self.open.store(false, Ordering::Release);
        self.from.wake();
    }
}

/// A line with nothing at the far end, for a run that must not talk to one.
#[derive(Debug, Default)]
pub struct Quiet;

impl Line for Quiet {
    fn write(&self, _: &[u8]) {}

    fn read(&self, into: &mut Vec<u8>, timeout: Duration) {
        into.clear();
        std::thread::sleep(timeout.min(Duration::from_millis(50)));
    }

    fn pending_output(&self) -> usize {
        0
    }

    fn is_open(&self) -> bool {
        true
    }
}
