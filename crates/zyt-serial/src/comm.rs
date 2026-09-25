//! The line as Windows holds it, through the `serialport` crate.
//!
//! This is the one module that depends on that crate, and the manifest names it
//! for this platform only. Linux talks to its descriptor itself.
//!
//! Windows drives `RTS` and `DTR` and cannot read either of them back, so the
//! two are set, remembered and reported as unknown; the four a peer drives are
//! read one call each, which is what the platform offers.

use crate::backend::PortHandle;
use crate::error::{PortError, Result};
use crate::lines::ControlLines;
use crate::params::{DataBits, FlowControl, LineParams, Parity, StopBits};
use std::time::Duration;

/// Opens one port and hands it over as the handle the supervisor drives.
pub(crate) fn open(
    path: &str,
    params: &LineParams,
    read_timeout: Duration,
) -> Result<Box<dyn PortHandle>> {
    let port = serialport::new(path, params.baud_rate)
        .data_bits(params.data_bits.into())
        .parity(params.parity.into())
        .stop_bits(params.stop_bits.into())
        .flow_control(params.flow_control.into())
        .timeout(read_timeout)
        .open()
        .map_err(|source| driver_error(path, source))?;

    Ok(Box::new(CommPort {
        path: path.to_string(),
        port,
        rts: false,
        dtr: false,
    }))
}

/// Maps a driver error onto this crate's errors. The crate carries an I/O kind
/// inside its own, so the mapping is the one every platform shares.
pub(crate) fn driver_error(path: &str, source: serialport::Error) -> PortError {
    PortError::from_driver(path, std::io::Error::from(source))
}

/// One open port, held by the driver crate.
struct CommPort {
    /// Device name the port was opened from, carried for the error variants
    /// that name it.
    path: String,
    /// The open port.
    port: Box<dyn serialport::SerialPort>,
    /// Request To Send, as this side last asked for it.
    rts: bool,
    /// Data Terminal Ready, as this side last asked for it.
    dtr: bool,
}

impl CommPort {
    /// Reads one input line. A driver that does not support a line reports it
    /// as down rather than failing the whole snapshot.
    fn read_line(
        &mut self,
        read: impl Fn(&mut Box<dyn serialport::SerialPort>) -> serialport::Result<bool>,
    ) -> Result<bool> {
        match read(&mut self.port) {
            Ok(level) => Ok(level),
            Err(source) => {
                let error = driver_error(&self.path, source);
                if error.is_fatal_for_connection() {
                    Err(error)
                } else {
                    Ok(false)
                }
            }
        }
    }
}

impl PortHandle for CommPort {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        use std::io::Read;
        match self.port.read(buf) {
            Ok(count) => Ok(count),
            Err(source) => match PortError::from_io(source) {
                PortError::Timeout => Ok(0),
                other => Err(other),
            },
        }
    }

    /// Hands what the driver takes now and says how much that was.
    ///
    /// The handle carries one timeout for reads and writes, so a write that
    /// waits for a slow line would fail after that timeout — and `write_all`
    /// would lose whatever it had already written. What the driver did not take
    /// stays with the caller, and what it took but has not sent yet is reported
    /// by [`PortHandle::pending_write`].
    fn write_some(&mut self, data: &[u8]) -> Result<usize> {
        use std::io::Write;
        match self.port.write(data) {
            Ok(count) => Ok(count),
            Err(source)
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(source) => Err(PortError::from_io(source)),
        }
    }

    fn pending_write(&mut self) -> Result<usize> {
        match self.port.bytes_to_write() {
            Ok(count) => Ok(count as usize),
            Err(source) => {
                let error = driver_error(&self.path, source);
                if error.is_fatal_for_connection() {
                    Err(error)
                } else {
                    Ok(0)
                }
            }
        }
    }

    /// Reads the four lines a peer drives. The two this side drives are
    /// reported as unknown: the platform has no call that reads them back, and
    /// the caller falls back to what was asked for.
    fn lines(&mut self) -> Result<ControlLines> {
        Ok(ControlLines {
            rts_asked: self.rts,
            dtr_asked: self.dtr,
            rts: None,
            dtr: None,
            cts: self.read_line(|port| port.read_clear_to_send())?,
            dsr: self.read_line(|port| port.read_data_set_ready())?,
            cd: self.read_line(|port| port.read_carrier_detect())?,
            ri: self.read_line(|port| port.read_ring_indicator())?,
        })
    }

    fn set_rts(&mut self, level: bool) -> Result<()> {
        self.port
            .write_request_to_send(level)
            .map_err(|source| driver_error(&self.path, source))?;
        self.rts = level;
        Ok(())
    }

    fn set_dtr(&mut self, level: bool) -> Result<()> {
        self.port
            .write_data_terminal_ready(level)
            .map_err(|source| driver_error(&self.path, source))?;
        self.dtr = level;
        Ok(())
    }

    fn set_params(&mut self, params: &LineParams) -> Result<()> {
        let path = self.path.clone();
        let map = |source| driver_error(&path, source);
        self.port.set_baud_rate(params.baud_rate).map_err(map)?;
        self.port
            .set_data_bits(params.data_bits.into())
            .map_err(map)?;
        self.port.set_parity(params.parity.into()).map_err(map)?;
        self.port
            .set_stop_bits(params.stop_bits.into())
            .map_err(map)?;
        self.port
            .set_flow_control(params.flow_control.into())
            .map_err(map)?;
        Ok(())
    }
}

impl From<DataBits> for serialport::DataBits {
    fn from(value: DataBits) -> Self {
        match value {
            DataBits::Five => Self::Five,
            DataBits::Six => Self::Six,
            DataBits::Seven => Self::Seven,
            DataBits::Eight => Self::Eight,
        }
    }
}

impl From<Parity> for serialport::Parity {
    fn from(value: Parity) -> Self {
        match value {
            Parity::None => Self::None,
            Parity::Odd => Self::Odd,
            Parity::Even => Self::Even,
        }
    }
}

impl From<StopBits> for serialport::StopBits {
    fn from(value: StopBits) -> Self {
        match value {
            StopBits::One => Self::One,
            StopBits::Two => Self::Two,
        }
    }
}

impl From<FlowControl> for serialport::FlowControl {
    fn from(value: FlowControl) -> Self {
        match value {
            FlowControl::None => Self::None,
            FlowControl::Software => Self::Software,
            FlowControl::Hardware => Self::Hardware,
        }
    }
}
