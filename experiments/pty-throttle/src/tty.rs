//! The terminal this runs in, and the pseudo terminal the command runs in.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command};

use rustix::fs::{Mode, OFlags};
use rustix::pty::OpenptFlags;
use rustix::termios::{OptionalActions, SpecialCodeIndex, Termios, Winsize};

/// What a program leaves switched on in a terminal when it is cut off half
/// way: the alternate screen, a hidden cursor, mouse reports, bracketed paste,
/// focus reports, the keys of the application mode, a scroll region, a
/// character set, attributes.
pub const RESET: &[u8] = b"\x1b[?1049l\x1b[?47l\x1b[?1047l\x1b[?25h\
\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\
\x1b[?2004l\x1b[?1004l\x1b[?1l\x1b>\x1b[r\x1b(B\x0f\x1b[m\r\n";

/// The size of a window when the terminal this runs in says none.
pub const FALLBACK_WINDOW: Winsize = Winsize {
    ws_row: 24,
    ws_col: 80,
    ws_xpixel: 0,
    ws_ypixel: 0,
};

/// The settings of the terminal this runs in, put back when the value goes.
pub struct Raw<Fd: AsFd> {
    fd: Fd,
    saved: Termios,
}

impl<Fd: AsFd> Raw<Fd> {
    /// Switches the terminal to raw mode: no echo, no line editing, no signals
    /// from keys, no translation of bytes either way, a read returning every
    /// byte as it comes. Every byte a key sends goes on to the command — `^C`,
    /// `^Z`, `^S` and `^Q` included — and the terminal of the command does with
    /// it what its own settings say.
    pub fn enter(fd: Fd, saved: Termios) -> io::Result<Self> {
        let mut raw = saved.clone();
        raw.make_raw();
        raw.special_codes[SpecialCodeIndex::VMIN] = 1;
        raw.special_codes[SpecialCodeIndex::VTIME] = 0;
        rustix::termios::tcsetattr(&fd, OptionalActions::Flush, &raw)?;
        Ok(Self { fd, saved })
    }
}

impl<Fd: AsFd> Drop for Raw<Fd> {
    fn drop(&mut self) {
        let _ = rustix::termios::tcsetattr(&self.fd, OptionalActions::Flush, &self.saved);
    }
}

/// The settings of a terminal, or `None` when it is no terminal.
pub fn settings(fd: impl AsFd) -> Option<Termios> {
    rustix::termios::tcgetattr(fd).ok()
}

/// The window size of a terminal that says one.
pub fn window(fd: impl AsFd) -> Option<Winsize> {
    rustix::termios::tcgetwinsize(fd)
        .ok()
        .filter(|size| size.ws_col > 0 && size.ws_row > 0)
}

/// Tells the pseudo terminal its size; the kernel sends `SIGWINCH` to the
/// command when it changed.
pub fn set_window(fd: impl AsFd, size: Winsize) {
    let _ = rustix::termios::tcsetwinsize(fd, size);
}

/// The pseudo terminal of the command, its side kept here.
pub struct Pty {
    /// The side this reads and writes, not waiting.
    pub master: OwnedFd,
    /// The command, through [`lead`].
    pub child: Child,
    /// The speed the terminal of the command was told, if it took one.
    pub speed_told: Option<u32>,
}

/// Opens a pseudo terminal and starts `command` in it, through this program
/// started again as [`lead`].
///
/// The pseudo terminal starts from the settings of the outer terminal — the
/// erase key, the interrupt key, the flow control — or from the defaults of
/// the kernel when there is none, and is told the speed of the line, so `stty`
/// and a program asking for the speed (`ospeed`) read it. It is not told the
/// frame: a pseudo terminal of Linux keeps `cs8` and no parity whatever it is
/// told, so the frame lives in the timing and the lost bits of [`Line`].
///
/// [`Line`]: crate::line::Line
pub fn spawn(
    command: &[String],
    outer: Option<&Termios>,
    size: Winsize,
    baud: u32,
) -> io::Result<Pty> {
    let master =
        rustix::pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
    rustix::pty::grantpt(&master)?;
    rustix::pty::unlockpt(&master)?;
    let name = rustix::pty::ptsname(&master, Vec::new())?;
    let slave = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut termios = match outer {
        Some(outer) => outer.clone(),
        None => rustix::termios::tcgetattr(&slave)?,
    };
    let named = nearest_speed(baud);
    let speed_told = termios.set_speed(named).is_ok().then_some(named);
    rustix::termios::tcsetattr(&slave, OptionalActions::Now, &termios)?;
    rustix::termios::tcsetwinsize(&slave, size)?;
    rustix::io::ioctl_fionbio(&master, true)?;
    let child = Command::new(std::env::current_exe()?)
        .arg(LEAD)
        .args(command)
        .stdin(slave.try_clone()?)
        .stdout(slave.try_clone()?)
        .stderr(slave)
        .spawn()?;
    Ok(Pty {
        master,
        child,
        speed_told,
    })
}

/// The speeds `termios` has a name for. Linux takes any other as well, but
/// reads it back through `cfgetospeed` as `0`, which a program takes for a line
/// that hung up.
const NAMED_SPEEDS: &[u32] = &[
    50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
    115200, 230400, 460800, 500000, 576000, 921600, 1000000, 1152000, 1500000, 2000000, 2500000,
    3000000, 3500000, 4000000,
];

/// The named speed nearest to `baud`.
pub fn nearest_speed(baud: u32) -> u32 {
    NAMED_SPEEDS
        .iter()
        .copied()
        .min_by_key(|named| named.abs_diff(baud))
        .unwrap_or(baud)
}

/// The first argument that starts this program as [`lead`].
pub const LEAD: &str = "--lead-session";

/// Run in the child with the pseudo terminal on its standard input: makes a
/// session of its own, takes the pseudo terminal as its controlling terminal,
/// and becomes the command. A program needs both to get `SIGWINCH`, `^C` and
/// `/dev/tty`. A freshly started process leads no group, so `setsid` cannot
/// fail for it, and both calls are safe ones; done before `exec` by the
/// parent, they would need `pre_exec`, which is not.
pub fn lead(command: &[String]) -> io::Error {
    let Some((program, args)) = command.split_first() else {
        return io::Error::new(io::ErrorKind::InvalidInput, "no command");
    };
    if let Err(error) = rustix::process::setsid() {
        return error.into();
    }
    if let Err(error) = rustix::process::ioctl_tiocsctty(io::stdin()) {
        return error.into();
    }
    let error = Command::new(program).args(args).exec();
    io::Error::new(error.kind(), format!("{program}: {error}"))
}
