//! The line a screen reads the bytes from and writes its answers to.

/// A source as the screen sees it: bytes in, bytes out.
///
/// It is shared with the window, which drives the rest of the source — the
/// modem lines, the parameters, the stop — so it is called through `&self`.
pub trait Wire: Send + Sync {
    /// Moves what arrived into `spare`, which is cleared first.
    fn read_into(&self, spare: &mut Vec<u8>);

    /// Hands bytes to the line.
    fn write(&self, bytes: &[u8]);

    /// Bytes of this side not on the line yet.
    fn pending_output(&self) -> usize {
        0
    }

    /// Bytes that came and nobody took yet.
    fn waiting(&self) -> usize {
        0
    }

    /// True while the one who reads it owns the line (`crate::line`).
    fn owned(&self) -> bool {
        true
    }
}

impl Wire for zyt_serial::PortSupervisor {
    fn read_into(&self, spare: &mut Vec<u8>) {
        zyt_serial::PortSupervisor::read_into(self, spare);
    }

    fn write(&self, bytes: &[u8]) {
        zyt_serial::PortSupervisor::write(self, bytes);
    }

    fn pending_output(&self) -> usize {
        self.status().pending_output
    }

    fn waiting(&self) -> usize {
        self.read_buffer().0
    }
}

impl Wire for zyt_pty::PtySession {
    fn read_into(&self, spare: &mut Vec<u8>) {
        zyt_pty::PtySession::read_into(self, spare);
    }

    fn write(&self, bytes: &[u8]) {
        zyt_pty::PtySession::write(self, bytes);
    }

    fn pending_output(&self) -> usize {
        zyt_pty::PtySession::pending_output(self)
    }

    fn waiting(&self) -> usize {
        self.read_buffer().0
    }
}
