//! Abstraction over the operating system serial driver. The supervisor talks
//! only to these traits, so it can be tested with a fake device.
//!
//! One module per platform answers them: `tty` on Linux, `comm` on Windows.
//! Nothing here knows which of the two it was built with beyond the one import
//! below.

use crate::enumerate::{PortInfo, available_ports};
use crate::error::Result;
use crate::lines::ControlLines;
use crate::params::LineParams;
use std::time::Duration;

#[cfg(windows)]
use crate::comm::open as open_port;
#[cfg(target_os = "linux")]
use crate::tty::open as open_port;

/// Opens ports and lists them.
pub trait PortBackend: Send + 'static {
    /// Lists the ports currently present in the system.
    fn list(&self) -> Result<Vec<PortInfo>>;

    /// Opens one port with the given line parameters.
    fn open(&self, path: &str, params: &LineParams) -> Result<Box<dyn PortHandle>>;
}

/// One open port.
pub trait PortHandle: Send {
    /// Reads available bytes. Returns `Ok(0)` when the read timeout expired.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Writes all bytes.
    /// Hands as much of `data` to the driver as it takes right now and reports
    /// how much that was.
    ///
    /// A driver that cannot take anything before the write timeout expires
    /// answers `Ok(0)`: nothing was lost and the caller keeps the rest. A
    /// partial write is normal on a slow line, which is why this is not
    /// `write_all` — that one either writes everything or throws away what it
    /// already wrote.
    fn write_some(&mut self, data: &[u8]) -> Result<usize>;

    /// Reads the modem control lines.
    fn lines(&mut self) -> Result<ControlLines>;

    /// Drives the Request To Send line.
    fn set_rts(&mut self, level: bool) -> Result<()>;

    /// Drives the Data Terminal Ready line.
    fn set_dtr(&mut self, level: bool) -> Result<()>;

    /// Applies new line parameters to the open port.
    fn set_params(&mut self, params: &LineParams) -> Result<()>;

    /// Bytes the driver accepted but has not put on the line yet.
    fn pending_write(&mut self) -> Result<usize>;
}

/// Backend that uses the serial driver of the operating system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBackend {
    /// Read timeout of the underlying handle.
    pub read_timeout: Duration,
}

impl SystemBackend {
    /// Backend with a read timeout suited for interactive use.
    pub fn new() -> Self {
        Self {
            read_timeout: Duration::from_millis(20),
        }
    }
}

impl PortBackend for SystemBackend {
    fn list(&self) -> Result<Vec<PortInfo>> {
        available_ports()
    }

    fn open(&self, path: &str, params: &LineParams) -> Result<Box<dyn PortHandle>> {
        open_port(path, params, self.read_timeout)
    }
}
