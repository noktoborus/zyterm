//! Signals turned into bytes of a socket, so the loop that waits on the line
//! waits on them too.

use std::io::{self, Read as _};
use std::os::unix::net::UnixStream;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};

/// One socket for a change of the window, one for a signal that ends the run.
pub struct Signals {
    /// Readable when the window of the terminal changed.
    pub resized: UnixStream,
    /// Readable when `SIGTERM`, `SIGHUP` or `SIGINT` came.
    pub ended: UnixStream,
}

impl Signals {
    /// Installs the handlers.
    pub fn install() -> io::Result<Self> {
        let resized = watch(&[SIGWINCH])?;
        let ended = watch(&[SIGTERM, SIGHUP, SIGINT])?;
        Ok(Self { resized, ended })
    }
}

/// A socket written to whenever one of `signals` comes.
fn watch(signals: &[Signal]) -> io::Result<UnixStream> {
    let (reader, writer) = UnixStream::pair()?;
    reader.set_nonblocking(true)?;
    for &signal in signals {
        signal_hook::low_level::pipe::register(signal, writer.try_clone()?)?;
    }
    Ok(reader)
}

/// Empties a socket of [`Signals`]; true when anything was in it.
pub fn take(mut socket: &UnixStream) -> bool {
    let mut bytes = [0u8; 64];
    let mut any = false;
    while let Ok(read @ 1..) = socket.read(&mut bytes) {
        any |= read > 0;
    }
    any
}

type Signal = std::ffi::c_int;
