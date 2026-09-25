//! Standard output, filled faster and faster, and what it cost to fill it.
//!
//! A terminal is easy to watch while somebody types at it and hard to watch
//! while a build log is pouring through it, and the second is where the reading
//! ladder of this application is decided. So this writes lines at a speed it
//! names, holds that speed for ten seconds, doubles it, and goes on until it is
//! writing ten megabytes a second.
//!
//! Every line carries its number and the speed it was written at, so what a
//! terminal dropped, reordered or fell behind on can be read off the screen
//! rather than guessed at.
//!
//! A reader that cannot keep up does not say so: it stops taking bytes, the pipe
//! fills, and the write of the program on the other end stops returning. That
//! wait is the delay, and it is the thing worth measuring — a step that asked
//! for ten megabytes a second and spent eight of its ten seconds inside `write`
//! is a terminal that gave up at some speed below the one it was asked for.
//!
//! It is said in two places. The first line written after a wait carries how
//! long that wait was, so the delay stands in the stream at the point where it
//! happened and is read on the screen along with everything else. Standard
//! error then carries one line per step: what it asked for, what it managed,
//! and how long it stood still altogether.
//!
//! It is primitive on purpose: no dependencies, one thread, two numbers on the
//! command line.

use std::io::Write as _;
use std::time::{Duration, Instant};

/// The speed it starts at, in bytes a second.
///
/// It is under the first step of the reading ladder, so the run begins where
/// nothing is held back at all.
const START: u64 = 1024;

/// The speed it stops after, in bytes a second.
const TOP: u64 = 10_000_000;

/// How long one speed is held before the next one.
const STEP: Duration = Duration::from_secs(10);

/// How often the writer looks at the clock inside one second.
///
/// It decides how lumpy the stream is and nothing else: what is owed is worked
/// out from the time that has passed, so a tick that is late is caught up with
/// rather than lost.
const TICKS: u32 = 20;

/// A write that takes longer than this was waiting for the reader.
///
/// A write nobody is holding back is a copy into a buffer and a system call,
/// which is microseconds. A millisecond is therefore not a slow write but a
/// write that stood still, and counting them apart is the whole point.
const STALL: Duration = Duration::from_millis(1);

/// What a line carries after its number and its speed, to give it a width.
const FILL: &str = "................................";

fn main() {
    let mut args = std::env::args().skip(1);
    let top = number(args.next()).unwrap_or(TOP);
    let step = number(args.next()).map_or(STEP, Duration::from_secs);

    let mut written = 0u64;
    let mut rate = START.min(top);
    loop {
        let Some(held) = hold(rate, step, &mut written) else {
            return;
        };
        report(rate, &held);
        if rate >= top {
            return;
        }
        rate = rate.saturating_mul(2).min(top);
    }
}

/// One number of the command line, when it is one.
fn number(text: Option<String>) -> Option<u64> {
    text?.parse().ok()
}

/// What one step came to.
struct Held {
    /// Bytes written during the step.
    written: u64,
    /// How long the step took, which is longer than it was given when the
    /// reader held the writes.
    took: Duration,
    /// Time spent inside a write that did not return at once.
    waited: Duration,
    /// The longest single write.
    longest: Duration,
    /// How many writes did not return at once.
    stalls: u32,
}

/// Writes at one speed for the length of a step, and says what it cost.
///
/// What is owed is the speed times the time that has passed since the step
/// began, so a write that took longer than its turn is made up for by the next
/// one and the speed over the whole step is the speed that was asked for — for
/// as long as the reader takes what it is given. Where it does not, the writes
/// stop returning, the step runs long, and that is what the answer carries.
///
/// A write that stood still is written down twice: on the first line that goes
/// out after it, where it is seen in the stream at the point it happened, and
/// in the tally this answers with.
///
/// Answers nothing when standard output has gone, which is what a reader that
/// closed the pipe looks like from here.
fn hold(rate: u64, step: Duration, written: &mut u64) -> Option<Held> {
    let out = std::io::stdout();
    let mut out = out.lock();
    let tick = Duration::from_secs_f64(1.0 / f64::from(TICKS));
    let began = Instant::now();

    let mut held = Held {
        written: 0,
        took: Duration::ZERO,
        waited: Duration::ZERO,
        longest: Duration::ZERO,
        stalls: 0,
    };
    let mut piece: Vec<u8> = Vec::new();
    let mut delayed: Option<Duration> = None;

    while began.elapsed() < step {
        let owed = (rate as f64 * began.elapsed().as_secs_f64()) as u64;
        piece.clear();
        while held.written + piece.len() as u64 <= owed {
            *written += 1;
            let mut line = format!("{written:08} {rate:>9} B/s {FILL}");
            if let Some(waited) = delayed.take() {
                line.push_str(&format!(" held back {} ms", waited.as_millis()));
            }
            line.push('\n');
            piece.extend_from_slice(line.as_bytes());
        }

        if !piece.is_empty() {
            let started = Instant::now();
            out.write_all(&piece).ok()?;
            out.flush().ok()?;
            let took = started.elapsed();

            held.written += piece.len() as u64;
            if took >= STALL {
                held.waited += took;
                held.longest = held.longest.max(took);
                held.stalls += 1;
                delayed = Some(took);
            }
        }

        std::thread::sleep(tick);
    }

    held.took = began.elapsed();
    Some(held)
}

/// Writes down what one step came to, on standard error so that it is not part
/// of the stream it is about.
fn report(rate: u64, held: &Held) {
    let seconds = held.took.as_secs_f64().max(f64::MIN_POSITIVE);
    let managed = held.written as f64 / seconds;
    let mut line = format!(
        "{rate:>9} B/s asked, {:>9.0} B/s managed, {:>8} bytes in {seconds:>5.1} s",
        managed, held.written
    );
    if held.stalls > 0 {
        line.push_str(&format!(
            ", held back {:>5.1} s in {} writes, longest {} ms",
            held.waited.as_secs_f64(),
            held.stalls,
            held.longest.as_millis()
        ));
    }
    eprintln!("{line}");
}
