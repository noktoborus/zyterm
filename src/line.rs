//! The line of the session: one owner at a time, the screen or a run, and the
//! counters of what went over it.
//!
//! ```text
//!  source ──Notify──► Line ──wake of the owner──► screen, or a run
//!     rx ◄──read_into── Lease of the owner
//!     tx ◄──write────── Lease of the owner
//! ```
//!
//! The screen owns the line until a run with `hold_line` takes it, and again
//! once the run gives it back. A lease that does not own the line reads
//! nothing and drops what it writes. `take`, `give_back` and each read and
//! write are under one lock, so a portion is never split between owners.
//! Every byte is counted here, whoever owns the line.

use crate::screen::{Wake, Wire};
use crate::session::Moment;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A function that wakes an owner.
type Ring = Arc<dyn Fn() + Send + Sync>;
/// What gives the line and the grid the size of a far terminal; true when
/// the line took it.
pub type Resize = Arc<dyn Fn(u16, u16) -> bool + Send + Sync>;

/// Who owns the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    Screen,
    Run,
}

/// What went over the line since the source was attached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Bytes that came from the line.
    pub bytes_in: u64,
    /// Bytes that came since this side last wrote.
    pub answered: u64,
    /// Bytes that went to the line.
    pub bytes_out: u64,
    /// When the last bytes came.
    pub last_data: Option<Moment>,
    /// When the first bytes came since this side last wrote.
    pub first_data: Option<Moment>,
    /// When this side last wrote.
    pub last_written: Option<Moment>,
}

/// What the lock of the line holds.
struct State {
    wire: Option<Arc<dyn Wire>>,
    owner: Owner,
    screen: Ring,
    run: Option<Ring>,
    counters: Counters,
}

/// The line of the session.
pub struct Line {
    state: Mutex<State>,
}

impl Line {
    /// A line with no source, owned by the screen that `screen` wakes.
    pub fn new(screen: Ring) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                wire: None,
                owner: Owner::Screen,
                screen,
                run: None,
                counters: Counters::default(),
            }),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// What the source calls when bytes came: it wakes the owner.
    pub fn notify(self: &Arc<Self>) -> Ring {
        let line = Arc::downgrade(self);
        Arc::new(move || {
            let Some(line) = line.upgrade() else {
                return;
            };
            let ring = {
                let state = line.state();
                match (state.owner, &state.run) {
                    (Owner::Run, Some(run)) => run.clone(),
                    _ => state.screen.clone(),
                }
            };
            ring();
        })
    }

    /// Puts a source on the line, or none, and starts the counters again.
    pub fn attach(&self, wire: Option<Arc<dyn Wire>>) {
        let mut state = self.state();
        state.wire = wire;
        state.counters = Counters::default();
    }

    /// The counters, as they stand.
    pub fn counters(&self) -> Counters {
        self.state().counters
    }

    /// Forgets when the bytes came and went; the counts stay.
    pub fn forget_times(&self) {
        let counters = &mut self.state().counters;
        counters.last_data = None;
        counters.first_data = None;
        counters.last_written = None;
    }

    /// The lease of the screen.
    pub fn screen(self: &Arc<Self>) -> Arc<dyn Wire> {
        Arc::new(Lease {
            line: self.clone(),
            owner: Owner::Screen,
        })
    }

    /// The line of a run. One that holds the line takes it from the screen;
    /// one that does not is closed from the start. `resize` sizes the line
    /// for it, and `port` is the serial port behind it, when there is one.
    pub fn run(
        self: &Arc<Self>,
        holds: bool,
        resize: Resize,
        port: Option<crate::port::ScriptPort>,
    ) -> Arc<RunLine> {
        let wake = Arc::new(Bell::default());
        let line = Arc::new(RunLine {
            lease: Lease {
                line: self.clone(),
                owner: Owner::Run,
            },
            bell: wake.clone(),
            open: AtomicBool::new(holds),
            resize,
            port,
        });
        if holds {
            let mut state = self.state();
            state.owner = Owner::Run;
            state.run = Some(Arc::new(move || wake.ring()));
        }
        line
    }

    /// Gives the line back to the screen, and wakes it for what is left.
    fn give_back(&self) {
        let screen = {
            let mut state = self.state();
            state.owner = Owner::Screen;
            state.run = None;
            state.screen.clone()
        };
        screen();
    }
}

/// A hold on the line: it reads and writes while its owner owns the line.
struct Lease {
    line: Arc<Line>,
    owner: Owner,
}

impl Wire for Lease {
    fn read_into(&self, spare: &mut Vec<u8>) {
        spare.clear();
        let mut state = self.line.state();
        let Some(wire) = state.wire.clone().filter(|_| state.owner == self.owner) else {
            return;
        };
        wire.read_into(spare);
        if spare.is_empty() {
            return;
        }
        let counters = &mut state.counters;
        let count = spare.len() as u64;
        let now = Moment::now();
        counters.bytes_in += count;
        counters.answered += count;
        counters.last_data = Some(now);
        counters.first_data.get_or_insert(now);
    }

    fn write(&self, bytes: &[u8]) {
        let mut state = self.line.state();
        if bytes.is_empty() || state.owner != self.owner {
            return;
        }
        if let Some(wire) = &state.wire {
            wire.write(bytes);
        }
        let counters = &mut state.counters;
        counters.bytes_out += bytes.len() as u64;
        counters.last_written = Some(Moment::now());
        counters.first_data = None;
        counters.answered = 0;
    }

    fn pending_output(&self) -> usize {
        self.line
            .state()
            .wire
            .as_ref()
            .map_or(0, |wire| wire.pending_output())
    }

    fn waiting(&self) -> usize {
        self.line
            .state()
            .wire
            .as_ref()
            .map_or(0, |wire| wire.waiting())
    }

    fn owned(&self) -> bool {
        self.line.state().owner == self.owner
    }
}

/// What wakes a run: its own wait, and the wait of `zyt.wait`.
#[derive(Default)]
struct Bell {
    wake: Wake,
    also: Mutex<Option<Ring>>,
}

impl Bell {
    fn ring(&self) {
        self.wake.ring();
        let also = self
            .also
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if let Some(also) = also {
            also();
        }
    }
}

/// The line as a run sees it.
pub struct RunLine {
    lease: Lease,
    bell: Arc<Bell>,
    open: AtomicBool,
    resize: Resize,
    port: Option<crate::port::ScriptPort>,
}

impl RunLine {
    /// The port, while the run holds the line.
    fn port(&self) -> Option<&crate::port::ScriptPort> {
        self.port
            .as_ref()
            .filter(|_| self.open.load(Ordering::Acquire))
    }
}

impl zyt_script::Line for RunLine {
    fn port(&self) -> Option<zyt_script::PortParams> {
        RunLine::port(self).map(crate::port::ScriptPort::params)
    }

    fn set_port(&self, params: &zyt_script::PortParams) -> Result<zyt_script::PortParams, String> {
        match RunLine::port(self) {
            Some(port) => port.set_params(params),
            None => Err("no serial port".to_string()),
        }
    }

    fn switch_port(&self, switch: zyt_script::PortSwitch) -> bool {
        RunLine::port(self).is_some_and(|port| port.switch(switch))
    }

    fn port_lines(&self) -> Option<zyt_script::PortLines> {
        RunLine::port(self).map(crate::port::ScriptPort::lines)
    }

    fn resize(&self, columns: u16, rows: u16) -> bool {
        self.is_open() && columns > 0 && rows > 0 && (self.resize)(columns, rows)
    }

    fn write(&self, bytes: &[u8]) {
        if self.is_open() {
            self.lease.write(bytes);
        }
    }

    fn read(&self, into: &mut Vec<u8>, timeout: Duration) {
        into.clear();
        let deadline = Instant::now() + timeout;
        while self.is_open() {
            self.lease.read_into(into);
            let left = deadline.saturating_duration_since(Instant::now());
            if !into.is_empty() || left.is_zero() {
                return;
            }
            self.bell.wake.wait(left);
        }
    }

    fn pending_output(&self) -> usize {
        self.lease.pending_output()
    }

    fn waiting(&self) -> usize {
        if self.is_open() {
            self.lease.waiting()
        } else {
            0
        }
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Shuts the line of the run and gives it back to the screen; a second
    /// close does nothing, so a run given up on takes no line from the next.
    fn close(&self) {
        if self.open.swap(false, Ordering::AcqRel) {
            self.lease.line.give_back();
        }
        self.bell.wake.ring();
    }

    fn set_wake(&self, wake: Option<Ring>) {
        *self
            .bell
            .also
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = wake;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyt_script::Line as _;

    /// A source that hands over what a test put in it and keeps what was
    /// written.
    #[derive(Default)]
    struct Fake {
        incoming: Mutex<Vec<u8>>,
        written: Mutex<Vec<u8>>,
    }

    impl Wire for Fake {
        fn read_into(&self, spare: &mut Vec<u8>) {
            spare.append(&mut self.incoming.lock().unwrap());
        }

        fn write(&self, bytes: &[u8]) {
            self.written.lock().unwrap().extend_from_slice(bytes);
        }
    }

    fn line() -> (Arc<Line>, Arc<Fake>) {
        let line = Line::new(Arc::new(|| {}));
        let fake = Arc::new(Fake::default());
        line.attach(Some(fake.clone()));
        (line, fake)
    }

    /// The screen reads and writes until a run takes the line, and again
    /// once it gives the line back; the run reads and writes in between.
    #[test]
    fn one_owner_reads_and_writes_at_a_time() {
        let (line, fake) = line();
        let screen = line.screen();
        let mut spare = Vec::new();

        let run = line.run(true, Arc::new(|_, _| false), None);
        fake.incoming
            .lock()
            .unwrap()
            .extend_from_slice(b"for the run");
        screen.read_into(&mut spare);
        assert!(spare.is_empty(), "the screen reads nothing");
        screen.write(b"key");
        run.read(&mut spare, Duration::ZERO);
        assert_eq!(spare, b"for the run");
        run.write(b"from the run");
        assert_eq!(*fake.written.lock().unwrap(), b"from the run");

        run.close();
        fake.incoming.lock().unwrap().extend_from_slice(b"left");
        screen.read_into(&mut spare);
        assert_eq!(spare, b"left", "the screen owns the line again");
        run.write(b"late");
        assert_eq!(*fake.written.lock().unwrap(), b"from the run");
    }

    /// A run that does not hold the line takes nothing from the screen.
    #[test]
    fn a_run_that_does_not_hold_the_line_takes_nothing() {
        let (line, fake) = line();
        let run = line.run(false, Arc::new(|_, _| false), None);
        fake.incoming.lock().unwrap().extend_from_slice(b"x");
        let mut spare = Vec::new();
        run.read(&mut spare, Duration::ZERO);
        assert!(spare.is_empty());
        line.screen().read_into(&mut spare);
        assert_eq!(spare, b"x");
    }

    /// The source wakes the owner, and a read of the run waits for it.
    #[test]
    fn the_source_wakes_the_read_of_the_run() {
        let (line, fake) = line();
        let run = line.run(true, Arc::new(|_, _| false), None);
        let notify = line.notify();
        let pushed = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            fake.incoming.lock().unwrap().extend_from_slice(b"late");
            notify();
        });
        let mut spare = Vec::new();
        let started = Instant::now();
        run.read(&mut spare, Duration::from_secs(5));
        pushed.join().unwrap();
        assert_eq!(spare, b"late");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    /// The counters count every owner; a write starts the answer again.
    #[test]
    fn the_counters_count_every_owner() {
        let (line, fake) = line();
        let screen = line.screen();
        let mut spare = Vec::new();
        fake.incoming.lock().unwrap().extend_from_slice(b"12345");
        screen.read_into(&mut spare);
        let run = line.run(true, Arc::new(|_, _| false), None);
        run.write(b"ab");
        fake.incoming.lock().unwrap().extend_from_slice(b"c");
        run.read(&mut spare, Duration::ZERO);

        let counters = line.counters();
        assert_eq!(
            (counters.bytes_in, counters.bytes_out, counters.answered),
            (6, 2, 1)
        );
        assert!(counters.last_written.is_some() && counters.first_data.is_some());
        line.attach(None);
        assert_eq!(line.counters(), Counters::default());
    }
}
