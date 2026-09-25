//! Bytes from the device, with a limit on how long to wait for them.
//!
//! A console line has no end of file: a device that stops answering leaves a
//! plain read waiting for ever. The bytes are therefore collected by a thread
//! of their own and handed over through a channel, which is what gives the
//! wait a deadline.

use crate::error::{Result, ShXferError};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

/// How much is taken from the line at a time.
const CHUNK: usize = 8192;

/// The bytes the device sends, buffered and bounded by a deadline.
pub struct Reader {
    chunks: Receiver<std::io::Result<Vec<u8>>>,
    waiting: VecDeque<u8>,
    ended: bool,
    timeout: Duration,
}

impl Reader {
    /// Starts reading `source` on a thread of its own.
    pub fn new(mut source: impl Read + Send + 'static, timeout: Duration) -> Self {
        let (sender, chunks) = channel();
        std::thread::spawn(move || {
            let mut buffer = vec![0_u8; CHUNK];
            loop {
                match source.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if sender.send(Ok(buffer[..count].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });

        Self {
            chunks,
            waiting: VecDeque::new(),
            ended: false,
            timeout,
        }
    }

    /// One line, without the carriage return and the newline that ended it.
    ///
    /// A line that never ends is not a line: the device that sends one is
    /// answered with the same complaint as a device that sends nothing.
    pub fn line(&mut self) -> Result<String> {
        loop {
            if let Some(at) = self.waiting.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.waiting.drain(..=at).collect();
                let text = String::from_utf8_lossy(&line);
                return Ok(text.trim_end_matches(['\n', '\r']).to_string());
            }
            if self.ended {
                if self.waiting.is_empty() {
                    return Err(self.quiet());
                }
                let line: Vec<u8> = self.waiting.drain(..).collect();
                return Ok(String::from_utf8_lossy(&line).trim_end().to_string());
            }
            self.fill()?;
        }
    }

    /// Exactly `count` bytes, however many lines they span.
    pub fn exact(&mut self, count: usize) -> Result<Vec<u8>> {
        while self.waiting.len() < count {
            if self.ended {
                return Ok(self.waiting.drain(..).collect());
            }
            self.fill()?;
        }
        Ok(self.waiting.drain(..count).collect())
    }

    /// The lines that are already here, without waiting for one more.
    ///
    /// A console that echoes sends back everything the client wrote. Nothing
    /// of it is wanted, but it has to be taken off the pile, or the pile grows
    /// by the size of the file being written.
    pub fn take_buffered_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(at) = self.waiting.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.waiting.drain(..=at).collect();
            let text = String::from_utf8_lossy(&line);
            lines.push(text.trim_end_matches(['\n', '\r']).to_string());
        }
        lines
    }

    fn fill(&mut self) -> Result<()> {
        match self.chunks.recv_timeout(self.timeout) {
            Ok(Ok(chunk)) => {
                self.waiting.extend(chunk);
                Ok(())
            }
            Ok(Err(source)) => Err(ShXferError::Line { source }),
            Err(RecvTimeoutError::Timeout) => Err(self.quiet()),
            Err(RecvTimeoutError::Disconnected) => {
                self.ended = true;
                Ok(())
            }
        }
    }

    fn quiet(&self) -> ShXferError {
        ShXferError::Quiet {
            seconds: self.timeout.as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(text: &'static str) -> Reader {
        Reader::new(std::io::Cursor::new(text), Duration::from_secs(1))
    }

    #[test]
    fn lines_come_without_their_endings() {
        let mut reader = reader("one\r\ntwo\nthree");
        assert_eq!(reader.line().expect("a line"), "one");
        assert_eq!(reader.line().expect("a line"), "two");
        assert_eq!(reader.line().expect("a line"), "three");
        assert!(reader.line().is_err());
    }

    #[test]
    fn bytes_are_taken_by_the_count() {
        let mut reader = reader("abcdef\n");
        assert_eq!(reader.exact(3).expect("bytes"), b"abc");
        assert_eq!(reader.line().expect("a line"), "def");
    }

    #[test]
    fn a_line_that_never_comes_is_not_waited_for_ever() {
        let (_sender, receiver) = channel::<Vec<u8>>();
        let source = ChannelReader { receiver };
        let mut reader = Reader::new(source, Duration::from_millis(50));
        assert!(matches!(
            reader.line(),
            Err(ShXferError::Quiet { seconds: 0 })
        ));
    }

    struct ChannelReader {
        receiver: Receiver<Vec<u8>>,
    }

    impl Read for ChannelReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            match self.receiver.recv() {
                Ok(chunk) => {
                    let count = chunk.len().min(buffer.len());
                    buffer[..count].copy_from_slice(&chunk[..count]);
                    Ok(count)
                }
                Err(_) => Ok(0),
            }
        }
    }
}
