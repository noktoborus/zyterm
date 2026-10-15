//! The loop that carries bytes between the terminal and the command.

use std::io::{self, Write as _};
use std::os::fd::AsFd;
use std::os::unix::process::ExitStatusExt as _;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::io::Errno;
use rustix::process::{Pid, Signal};

use crate::frame::Frame;
use crate::line::Line;
use crate::signals::{self, Signals};
use crate::tty::{self, Pty, Raw};

/// The key that leaves at once when pressed twice in a row: `^]`.
const QUIT: u8 = 0x1d;

/// What the command line says.
#[derive(clap::Parser)]
#[command(
    about = "Stands between this terminal and a command as a serial line of a given speed",
    after_help = "Examples:\n  pty-throttle -b 9600 -- tmux -CC new\n  pty-throttle -b 115200 -f 7E1 -- vim\n\n^] twice in a row leaves at once."
)]
pub struct Options {
    /// The speed of the line, in bits a second.
    #[arg(short, long, default_value_t = 9600, value_parser = clap::value_parser!(u32).range(1..))]
    baud: u32,
    /// The frame of a character: data bits 5 to 8, parity N, E or O, stop bits
    /// 1 or 2. Fewer than eight data bits lose the high bits, as on the wire.
    #[arg(short, long, default_value = "8N1")]
    frame: Frame,
    /// How many bytes each direction holds before the side writing into it
    /// waits, as the buffers of a tty driver do.
    #[arg(short, long, default_value_t = 4096, value_parser = clap::value_parser!(u32).range(1..))]
    queue: u32,
    /// Leaves the terminal as the command left it: no alternate screen left,
    /// no cursor shown, no mouse switched off at the end.
    #[arg(long)]
    no_reset: bool,
    /// Says nothing before the command starts and after it ends.
    #[arg(long)]
    quiet: bool,
    /// The command and its arguments; `$SHELL` when none.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// Why the loop stopped.
enum End {
    /// The command closed its terminal; what waited went out first.
    Closed,
    /// `^]` twice, or a signal: what waited is thrown away.
    Cut(&'static str),
}

/// Runs the command behind the line and returns the code to exit with.
pub fn run(options: Options) -> io::Result<i32> {
    let command = if options.command.is_empty() {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())]
    } else {
        options.command.clone()
    };
    let outer = tty::settings(io::stdin());
    let window = outer_window().unwrap_or(tty::FALLBACK_WINDOW);
    let reset = !options.no_reset && tty::settings(io::stdout()).is_some();
    let signals = Signals::install()?;
    let mut pty = tty::spawn(&command, outer.as_ref(), window, options.baud)?;
    if !options.quiet {
        eprintln!("{}; ^]^] leaves", describe(&options, pty.speed_told));
    }
    let raw = outer
        .map(|saved| Raw::enter(io::stdin(), saved))
        .transpose()?;
    let started = Instant::now();
    let mut relay = Relay::new(&options, started);
    let end = relay.carry(&pty, &signals);
    if let Ok(End::Cut(_)) = end {
        let _ = rustix::process::kill_process_group(Pid::from_child(&pty.child), Signal::HUP);
    }
    let status = pty.child.wait();
    if reset {
        let _ = io::stdout().write_all(tty::RESET);
        let _ = io::stdout().flush();
    }
    drop(raw);
    let end = end?;
    if !options.quiet {
        relay.report(started, &end);
    }
    let status = status?;
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

/// The size of the terminal this runs in, from whichever side is one.
fn outer_window() -> Option<rustix::termios::Winsize> {
    tty::window(io::stdin()).or_else(|| tty::window(io::stdout()))
}

/// The line, as it is said before the command starts.
fn describe(options: &Options, speed_told: Option<u32>) -> String {
    let bytes = f64::from(options.baud) / options.frame.bits() as f64;
    let told = match speed_told {
        Some(told) if told != options.baud => {
            format!(" (the terminal of the command is told {told}, the nearest speed it names)")
        }
        Some(_) => String::new(),
        None => " (the terminal of the command took no speed)".into(),
    };
    format!(
        "pty-throttle: {} baud {}, {bytes:.0} bytes a second{told}",
        options.baud, options.frame
    )
}

/// The two directions of the line and what stands between them.
struct Relay {
    down: Line,
    up: Line,
    buffer: Vec<u8>,
    quit_pressed: bool,
}

/// Which side of the loop is still open, and what it waits for.
struct Sides {
    stdin_open: bool,
    master_open: bool,
    up_blocked: bool,
}

impl Relay {
    fn new(options: &Options, now: Instant) -> Self {
        let queue = options.queue as usize;
        Self {
            down: Line::new(options.baud, options.frame, queue, now),
            up: Line::new(options.baud, options.frame, queue, now),
            buffer: vec![0; queue],
            quit_pressed: false,
        }
    }

    /// Carries bytes until the command closes its terminal or the user leaves.
    ///
    /// `down` is from the command to the terminal, `up` from the keys to the
    /// command. Each side is read only while its direction has room, so a
    /// command writing faster than the line carries waits in its `write`, as it
    /// would on a serial console.
    fn carry(&mut self, pty: &Pty, signals: &Signals) -> io::Result<End> {
        let mut sides = Sides {
            stdin_open: true,
            master_open: true,
            up_blocked: false,
        };
        loop {
            self.deliver(pty, &mut sides)?;
            if !sides.master_open && self.down.is_empty() {
                return Ok(End::Closed);
            }
            let stdin = io::stdin();
            let reads_stdin = sides.stdin_open && self.up.room() > 0;
            let mut master_events = PollFlags::empty();
            if sides.master_open && self.down.room() > 0 {
                master_events |= PollFlags::IN;
            }
            if sides.master_open && sides.up_blocked {
                master_events |= PollFlags::OUT;
            }
            let mut waits = vec![
                PollFd::new(&signals.resized, PollFlags::IN),
                PollFd::new(&signals.ended, PollFlags::IN),
            ];
            let stdin_slot = reads_stdin.then(|| {
                waits.push(PollFd::new(&stdin, PollFlags::IN));
                waits.len() - 1
            });
            let master_slot = (!master_events.is_empty()).then(|| {
                waits.push(PollFd::new(&pty.master, master_events));
                waits.len() - 1
            });
            match rustix::event::poll(&mut waits, self.timeout(sides.up_blocked).as_ref()) {
                Ok(_) | Err(Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            let revents =
                |slot: Option<usize>| slot.map_or(PollFlags::empty(), |slot| waits[slot].revents());
            let (resized, ended) = (waits[0].revents(), waits[1].revents());
            let (stdin_ready, master_ready) = (revents(stdin_slot), revents(master_slot));
            drop(waits);
            if !resized.is_empty()
                && signals::take(&signals.resized)
                && let Some(size) = outer_window()
            {
                tty::set_window(&pty.master, size);
            }
            if !ended.is_empty() && signals::take(&signals.ended) {
                return Ok(End::Cut("a signal"));
            }
            let now = Instant::now();
            if !stdin_ready.is_empty() {
                let room = self.up.room();
                match read_some(&stdin, &mut self.buffer[..room])? {
                    Some(0) => sides.stdin_open = false,
                    Some(read) => {
                        if self.quit(read) {
                            return Ok(End::Cut("^]^]"));
                        }
                        self.up.push(&self.buffer[..read], now);
                    }
                    None => {}
                }
            }
            if master_ready.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
                let room = self.down.room();
                match read_some(&pty.master, &mut self.buffer[..room]) {
                    Ok(Some(0)) | Err(_) => {
                        sides.master_open = false;
                        self.up.clear();
                    }
                    Ok(Some(read)) => self.down.push(&self.buffer[..read], now),
                    Ok(None) => {}
                }
            }
        }
    }

    /// Gives each side the bytes that have arrived by now.
    fn deliver(&mut self, pty: &Pty, sides: &mut Sides) -> io::Result<()> {
        let now = Instant::now();
        let ready = self.down.ready(now);
        if ready > 0 {
            let mut stdout = io::stdout().lock();
            stdout.write_all(self.down.front(ready))?;
            stdout.flush()?;
            self.down.consume(ready);
        }
        let ready = self.up.ready(now);
        sides.up_blocked = false;
        if ready > 0 && sides.master_open {
            let written = write_some(&pty.master, self.up.front(ready))?.unwrap_or(0);
            self.up.consume(written);
            sides.up_blocked = written < ready;
        }
        Ok(())
    }

    /// How long the loop may wait before a byte is due; none while nothing
    /// waits. Bytes for a command that takes none wait for the terminal to
    /// take them, not for the time.
    fn timeout(&self, up_blocked: bool) -> Option<Timespec> {
        let up = self.up.next().filter(|_| !up_blocked);
        let due = [self.down.next(), up].into_iter().flatten().min()?;
        Timespec::try_from(due.saturating_duration_since(Instant::now())).ok()
    }

    /// True on the second `^]` in a row among the first `read` bytes of the
    /// buffer.
    fn quit(&mut self, read: usize) -> bool {
        for &byte in &self.buffer[..read] {
            if byte == QUIT && self.quit_pressed {
                return true;
            }
            self.quit_pressed = byte == QUIT;
        }
        false
    }

    /// What went both ways, on standard error.
    fn report(&self, started: Instant, end: &End) {
        let seconds = started.elapsed().as_secs_f64();
        let rate = self.down.total() as f64 / seconds.max(f64::EPSILON);
        let ending = match end {
            End::Closed => "the command closed its terminal".to_string(),
            End::Cut(why) => format!("cut by {why}, what waited thrown away"),
        };
        eprintln!(
            "pty-throttle: {} bytes to the terminal in {seconds:.1} s, {rate:.0} bytes a second; {} bytes to the command; {ending}",
            self.down.total(),
            self.up.total()
        );
    }
}

/// A read that does not wait: `None` when nothing is there yet.
fn read_some(fd: impl AsFd, buffer: &mut [u8]) -> io::Result<Option<usize>> {
    match rustix::io::read(fd, buffer) {
        Ok(read) => Ok(Some(read)),
        Err(Errno::AGAIN | Errno::INTR) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// A write that does not wait: `None` when nothing was taken.
fn write_some(fd: impl AsFd, bytes: &[u8]) -> io::Result<Option<usize>> {
    match rustix::io::write(fd, bytes) {
        Ok(written) => Ok(Some(written)),
        Err(Errno::AGAIN | Errno::INTR) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
