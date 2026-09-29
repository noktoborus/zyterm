//! The Linux backend against a pseudo terminal.
//!
//! A pty is not a serial port — it has no modem lines and no speed — but it is
//! a device node that takes `termios` and carries bytes, which is every part of
//! the backend that does not need hardware: the claim on the node, the raw
//! line, the read timeout and what a read and a write hand over.

#![cfg(target_os = "linux")]

use std::ffi::CStr;
use std::time::Duration;
use zyt_serial::{LineParams, PortBackend, PortError, PortHandle, SystemBackend};

/// The far end of the pty, which stands in for the device.
struct Peer {
    /// The master descriptor.
    fd: libc::c_int,
    /// Path of the slave side, which is what the backend opens.
    path: String,
}

impl Peer {
    /// Opens a pty and names its slave side.
    fn new() -> Self {
        // Safety: each call takes the descriptor the one before it answered.
        let fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        assert!(fd >= 0, "{}", std::io::Error::last_os_error());
        assert_eq!(unsafe { libc::grantpt(fd) }, 0);
        assert_eq!(unsafe { libc::unlockpt(fd) }, 0);
        let path = unsafe { CStr::from_ptr(libc::ptsname(fd)) }
            .to_string_lossy()
            .to_string();
        Self { fd, path }
    }

    /// Puts bytes on the line, as a device would.
    fn send(&self, bytes: &[u8]) {
        // Safety: the buffer is read up to its length and belongs to the caller.
        let sent = unsafe { libc::write(self.fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(sent, bytes.len() as isize);
        std::thread::sleep(SETTLE);
    }

    /// Takes what arrived at the far end.
    fn receive(&self) -> Vec<u8> {
        std::thread::sleep(SETTLE);
        let mut buf = [0u8; 256];
        // Safety: the buffer is written up to its length and lives here.
        let got = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        buf[..got.max(0) as usize].to_vec()
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        // Safety: the descriptor is this one's and is closed once.
        unsafe { libc::close(self.fd) };
    }
}

/// How long the bytes are given to cross the pty before they are asked for.
const SETTLE: Duration = Duration::from_millis(50);

/// Opens the slave side of a pty with the default parameters.
fn open(peer: &Peer) -> Box<dyn PortHandle> {
    SystemBackend::new()
        .open(&peer.path, &LineParams::default())
        .expect("the slave side of a pty opens")
}

/// Nothing is done to a byte in either direction: a carriage return stays one,
/// a line feed is not grown a carriage return in front of it, and a byte that
/// would be a signal on a terminal is a byte here. Anything else would be a
/// second terminal in front of the one this program draws.
#[test]
fn the_line_passes_every_byte_through_untouched() {
    let peer = Peer::new();
    let mut port = open(&peer);

    peer.send(b"a\r\n\x03");
    let mut buf = [0u8; 64];
    let got = port.read(&mut buf).expect("the bytes arrive");
    assert_eq!(&buf[..got], b"a\r\n\x03");

    port.write_some(b"y\n").expect("the bytes go out");
    assert_eq!(peer.receive(), b"y\n");
}

/// A read with nothing on the line waits the read timeout out and says that
/// nothing came, rather than failing or blocking the worker for good.
#[test]
fn a_silent_line_times_out_and_is_not_an_error() {
    let peer = Peer::new();
    let mut port = open(&peer);

    let mut buf = [0u8; 64];
    assert_eq!(port.read(&mut buf).expect("silence is not a failure"), 0);
    assert_eq!(port.pending_write().expect("nothing waits"), 0);
}

/// Opening a port claims the device node: the second caller is told it is busy
/// instead of sharing the line with the first.
#[test]
fn an_open_port_is_claimed_against_a_second_open() {
    let peer = Peer::new();
    let port = open(&peer);

    let refused = SystemBackend::new()
        .open(&peer.path, &LineParams::default())
        .err()
        .expect("the node is taken");
    assert!(matches!(refused, PortError::Busy { .. }), "{refused:?}");

    drop(port);
    SystemBackend::new()
        .open(&peer.path, &LineParams::default())
        .expect("the closed handle gave the node back");
}

/// New parameters reach an open port, and the line keeps carrying bytes after
/// they did.
#[test]
fn parameters_are_changed_on_the_open_port() {
    let peer = Peer::new();
    let mut port = open(&peer);

    port.set_params(&LineParams {
        baud_rate: 9600,
        data_bits: zyt_serial::DataBits::Seven,
        parity: zyt_serial::Parity::Even,
        stop_bits: zyt_serial::StopBits::Two,
        flow_control: zyt_serial::FlowControl::Hardware,
        ..LineParams::default()
    })
    .expect("the driver takes them");

    peer.send(b"still here");
    let mut buf = [0u8; 64];
    let got = port.read(&mut buf).expect("the line still carries");
    assert_eq!(&buf[..got], b"still here");
}

/// A device node that is not there is not a port, and the error names it.
#[test]
fn a_node_that_is_not_there_is_not_a_port() {
    let error = SystemBackend::new()
        .open("/dev/there-is-no-such-port", &LineParams::default())
        .err()
        .expect("nothing is there to open");

    assert!(matches!(error, PortError::NotFound { .. }), "{error:?}");
}
