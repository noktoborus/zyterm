//! The line as the Linux driver holds it: one descriptor, one `termios2` and a
//! handful of ioctls.
//!
//! Everything this crate asks of an open port is a call on that descriptor, so
//! the port is the descriptor and nothing beside it. The parameters go in as
//! `termios2`, which carries the speed as a number rather than as one of a
//! fixed set of constants, so a device asking for a rate no constant names is
//! opened like any other.
//!
//! The descriptor is opened without blocking and every read waits on `poll`
//! for the read timeout, because a worker that blocks in the driver answers
//! neither its commands nor the modem lines.

use crate::backend::PortHandle;
use crate::error::{PortError, Result};
use crate::lines::ControlLines;
use crate::params::{DataBits, FlowControl, LineParams, Parity, StopBits};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

/// Opens one port and hands it over as the handle the supervisor drives.
pub(crate) fn open(
    path: &str,
    params: &LineParams,
    read_timeout: Duration,
) -> Result<Box<dyn PortHandle>> {
    Ok(Box::new(TtyPort::open(path, params, read_timeout)?))
}

/// One open port, held by the descriptor it was opened as. Dropping it closes
/// the descriptor and releases the device node.
#[derive(Debug)]
struct TtyPort {
    /// Device node the port was opened from, carried for the error variants
    /// that name it.
    path: String,
    /// The open descriptor.
    fd: OwnedFd,
    /// How long a read waits for the first byte.
    read_timeout: Duration,
    /// Request To Send, as this side last asked for it.
    rts: bool,
    /// Data Terminal Ready, as this side last asked for it.
    dtr: bool,
}

impl TtyPort {
    /// Opens the device node and puts the line parameters on it.
    fn open(path: &str, params: &LineParams, read_timeout: Duration) -> Result<Self> {
        let name = CString::new(path).map_err(|_| PortError::NotFound {
            path: path.to_string(),
        })?;

        // Safety: the name is a NUL terminated path held for the whole call.
        let opened = unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if opened < 0 {
            return Err(PortError::from_driver(path, io::Error::last_os_error()));
        }

        // Safety: the descriptor was just opened and is owned by nothing else.
        let fd = unsafe { OwnedFd::from_raw_fd(opened) };
        let port = Self {
            path: path.to_string(),
            fd,
            read_timeout,
            rts: false,
            dtr: false,
        };
        port.claim()?;
        port.configure(params)?;
        Ok(port)
    }

    /// Claims the port for this process, so a second open is refused as busy
    /// instead of sharing the line with whoever asks for it next.
    ///
    /// Two claims are laid, because they are answered by different callers.
    /// `TIOCEXCL` is the kernel's: every later `open` of the node fails, asked
    /// for or not. The advisory lock is the one other programs take — `screen`,
    /// `picocom` and whatever else reaches for the same adapter — and it is
    /// only answered by a program that asks for it.
    fn claim(&self) -> Result<()> {
        self.request(libc::TIOCEXCL, std::ptr::null_mut::<libc::c_int>())?;

        // Safety: the descriptor is this port's and `flock` takes no pointer.
        let locked = unsafe { libc::flock(self.fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if locked != 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Err(PortError::Busy {
                    path: self.path.clone(),
                });
            }
            return Err(PortError::from_driver(&self.path, error));
        }
        Ok(())
    }

    /// Puts the line parameters on the open port.
    fn configure(&self, params: &LineParams) -> Result<()> {
        // Safety: `termios2` is plain data and TCGETS2 fills every field of it.
        let mut line: libc::termios2 = unsafe { std::mem::zeroed() };
        self.request(libc::TCGETS2, &mut line)?;

        let modes = Modes::of(params);
        line.c_iflag = modes.input;
        line.c_oflag = modes.output;
        line.c_cflag = (modes.control & !libc::CBAUD) | libc::BOTHER;
        line.c_lflag = modes.local;
        line.c_cc[libc::VMIN] = 0;
        line.c_cc[libc::VTIME] = 0;
        line.c_ispeed = params.baud_rate;
        line.c_ospeed = params.baud_rate;

        self.request(libc::TCSETS2, &mut line)
    }

    /// Runs one ioctl on the descriptor.
    ///
    /// The request decides what the argument points at, which is why this is
    /// generic: every caller below hands it the type its own request reads or
    /// writes, and a request that takes no argument is handed a null pointer.
    fn request<T>(&self, request: libc::Ioctl, argument: *mut T) -> Result<()> {
        // Safety: the descriptor is this port's, and each request used in this
        // module touches at most the one value the argument points at.
        let answered = unsafe { libc::ioctl(self.fd.as_raw_fd(), request as _, argument) };
        if answered != 0 {
            return Err(PortError::from_driver(
                &self.path,
                io::Error::last_os_error(),
            ));
        }
        Ok(())
    }

    /// Waits up to the read timeout for something to arrive.
    ///
    /// A hang up is not an answer of its own: bytes already in the driver are
    /// still read after the device went away, and the read that finds nothing
    /// left is the one that reports the loss.
    fn readable(&self) -> Result<bool> {
        let mut watch = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let milliseconds = self.read_timeout.as_millis().min(i32::MAX as u128) as libc::c_int;

        // Safety: one descriptor of this port is watched, and `poll` writes
        // only the `revents` of the entry it is given.
        let answered = unsafe { libc::poll(&mut watch, 1, milliseconds) };
        if answered < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(PortError::from_io(error));
        }

        if watch.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(PortError::Disconnected);
        }
        Ok(watch.revents & (libc::POLLIN | libc::POLLHUP) != 0)
    }

    /// Raises or drops one of the two lines this side drives.
    fn drive(&self, line: libc::c_int, level: bool) -> Result<()> {
        let mut bits = line;
        let request = if level {
            libc::TIOCMBIS
        } else {
            libc::TIOCMBIC
        };
        self.request(request, &mut bits)
    }
}

impl PortHandle for TtyPort {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() || !self.readable()? {
            return Ok(0);
        }

        // Safety: the buffer is the caller's and is written up to its length.
        let got = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        match got {
            0 => Err(PortError::Disconnected),
            got if got > 0 => Ok(got as usize),
            _ => match io::Error::last_os_error() {
                error
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    Ok(0)
                }
                error => Err(PortError::from_io(error)),
            },
        }
    }

    /// Hands what the driver takes now and says how much that was.
    ///
    /// The descriptor does not block, so a driver whose buffer is full answers
    /// nothing rather than waiting: what it did not take stays with the caller,
    /// and what it took but has not sent yet is reported by
    /// [`PortHandle::pending_write`].
    fn write_some(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        // Safety: the data is the caller's and is read up to its length.
        let taken = unsafe { libc::write(self.fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
        match taken {
            taken if taken >= 0 => Ok(taken as usize),
            _ => match io::Error::last_os_error() {
                error
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    Ok(0)
                }
                error => Err(PortError::from_io(error)),
            },
        }
    }

    fn pending_write(&mut self) -> Result<usize> {
        let mut waiting: libc::c_int = 0;
        self.request(libc::TIOCOUTQ, &mut waiting)?;
        Ok(waiting.max(0) as usize)
    }

    /// Reads all six lines in one call.
    ///
    /// The driver keeps them in one word, including the two this side drives,
    /// so a snapshot is one ioctl rather than one call per line. What was asked
    /// for is reported beside what the driver says: opening a port raises both
    /// `RTS` and `DTR` before anything has asked for anything.
    fn lines(&mut self) -> Result<ControlLines> {
        let mut bits: libc::c_int = 0;
        self.request(libc::TIOCMGET, &mut bits)?;

        Ok(ControlLines {
            rts_asked: self.rts,
            dtr_asked: self.dtr,
            rts: Some(bits & libc::TIOCM_RTS != 0),
            dtr: Some(bits & libc::TIOCM_DTR != 0),
            cts: bits & libc::TIOCM_CTS != 0,
            dsr: bits & libc::TIOCM_DSR != 0,
            cd: bits & libc::TIOCM_CAR != 0,
            ri: bits & libc::TIOCM_RNG != 0,
        })
    }

    fn set_rts(&mut self, level: bool) -> Result<()> {
        self.drive(libc::TIOCM_RTS, level)?;
        self.rts = level;
        Ok(())
    }

    fn set_dtr(&mut self, level: bool) -> Result<()> {
        self.drive(libc::TIOCM_DTR, level)?;
        self.dtr = level;
        Ok(())
    }

    fn set_params(&mut self, params: &LineParams) -> Result<()> {
        self.configure(params)
    }
}

impl Drop for TtyPort {
    /// Gives the claim back before the descriptor goes.
    ///
    /// Nothing documents the kernel as dropping `TIOCEXCL` on the last close,
    /// so it is dropped here by hand: a node that stayed claimed would be a
    /// port this program itself could not open again. The advisory lock needs
    /// no such care — it is documented as released when the descriptor closes.
    fn drop(&mut self) {
        let _ = self.request(libc::TIOCNXCL, std::ptr::null_mut::<libc::c_int>());
    }
}

/// The four flag words of a line, as the parameters ask for them.
struct Modes {
    /// What is done to a byte on its way in.
    input: libc::tcflag_t,
    /// What is done to a byte on its way out.
    output: libc::tcflag_t,
    /// The character format and the lines.
    control: libc::tcflag_t,
    /// What the line discipline does with a byte.
    local: libc::tcflag_t,
}

impl Modes {
    /// The flags of a port that passes every byte through untouched, carrying
    /// the character format and the flow control the parameters ask for.
    ///
    /// Each word is built from nothing rather than from what stood there: a
    /// terminal is what this program draws, and a driver that echoes, maps line
    /// endings or waits for a line to be finished is a second terminal in front
    /// of it.
    fn of(params: &LineParams) -> Self {
        let mut input: libc::tcflag_t = 0;
        let mut control: libc::tcflag_t = libc::CREAD | libc::CLOCAL;

        control |= match params.data_bits {
            DataBits::Five => libc::CS5,
            DataBits::Six => libc::CS6,
            DataBits::Seven => libc::CS7,
            DataBits::Eight => libc::CS8,
        };
        match params.parity {
            Parity::None => {}
            Parity::Even => control |= libc::PARENB,
            Parity::Odd => control |= libc::PARENB | libc::PARODD,
        }
        if matches!(params.stop_bits, StopBits::Two) {
            control |= libc::CSTOPB;
        }
        match params.flow_control {
            FlowControl::None => {}
            FlowControl::Software => input |= libc::IXON | libc::IXOFF,
            FlowControl::Hardware => control |= libc::CRTSCTS,
        }

        Self {
            input,
            output: 0,
            control,
            local: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The character format is what the parameters asked for, and nothing the
    /// line discipline would do to a byte is switched on.
    #[test]
    fn a_raw_line_carries_the_format_and_nothing_else() {
        let modes = Modes::of(&LineParams::default());

        assert_eq!(modes.control & libc::CSIZE, libc::CS8);
        assert_eq!(modes.control & libc::PARENB, 0);
        assert_eq!(modes.control & libc::CSTOPB, 0);
        assert_eq!(modes.control & libc::CRTSCTS, 0);
        assert_eq!(modes.input, 0);
        assert_eq!(modes.output, 0);
        assert_eq!(modes.local, 0, "no echo, no canonical mode, no signals");
        assert_ne!(modes.control & libc::CLOCAL, 0, "carrier is not waited for");
        assert_ne!(modes.control & libc::CREAD, 0);
    }

    /// Odd parity is even parity and one bit more, seven bits are seven bits,
    /// and two stop bits are asked for where one is not.
    #[test]
    fn the_format_reaches_the_control_word() {
        let modes = Modes::of(&LineParams {
            data_bits: DataBits::Seven,
            parity: Parity::Odd,
            stop_bits: StopBits::Two,
            ..LineParams::default()
        });

        assert_eq!(modes.control & libc::CSIZE, libc::CS7);
        assert_ne!(modes.control & libc::PARENB, 0);
        assert_ne!(modes.control & libc::PARODD, 0);
        assert_ne!(modes.control & libc::CSTOPB, 0);
    }

    /// The two kinds of flow control live in two different words, and asking
    /// for one never switches the other on.
    #[test]
    fn flow_control_goes_where_the_driver_reads_it() {
        let software = Modes::of(&LineParams {
            flow_control: FlowControl::Software,
            ..LineParams::default()
        });
        assert_eq!(software.input, libc::IXON | libc::IXOFF);
        assert_eq!(software.control & libc::CRTSCTS, 0);

        let hardware = Modes::of(&LineParams {
            flow_control: FlowControl::Hardware,
            ..LineParams::default()
        });
        assert_ne!(hardware.control & libc::CRTSCTS, 0);
        assert_eq!(hardware.input, 0);
    }

    /// A path that is no device node is refused rather than opened, and the
    /// error names the path so the caller can say which one.
    #[test]
    fn a_path_that_names_no_device_is_not_a_port() {
        let error = TtyPort::open(
            "/dev/there-is-no-such-port",
            &LineParams::default(),
            Duration::from_millis(1),
        )
        .expect_err("nothing is there to open");

        assert!(matches!(error, PortError::NotFound { .. }), "{error:?}");
    }

    /// A path carrying a NUL is no path at all, and it never reaches the
    /// driver.
    #[test]
    fn a_path_that_cannot_be_a_path_is_refused() {
        let error = TtyPort::open(
            "/dev/tty\0USB0",
            &LineParams::default(),
            Duration::from_millis(1),
        )
        .expect_err("the name is not a name");

        assert!(matches!(error, PortError::NotFound { .. }), "{error:?}");
    }
}
