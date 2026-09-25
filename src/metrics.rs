//! What this process costs: the processor it used, the memory it holds and the
//! frames it drew.
//!
//! The numbers are read from the operating system and not counted here, because
//! what an allocator holds and what the system gave it are two different things
//! and only the second one is what somebody watching a system monitor sees.
//!
//! Linux answers through `/proc/self`. Nothing else does, so on another platform
//! every number is `None` and the window that shows them says so: a made up
//! number is worse than an empty line.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How far back the frames are counted for the rate.
const OVER: Duration = Duration::from_secs(1);

/// What the process costs at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    /// Memory the system has given this process, in bytes.
    pub resident: Option<u64>,
    /// Most memory it ever held at once, in bytes.
    pub peak: Option<u64>,
    /// Address space it has asked for, in bytes.
    pub virtual_size: Option<u64>,
    /// Share of one processor used since the previous sample, where `1.0` is one
    /// core running without a pause.
    pub processor: Option<f32>,
    /// Threads the process is running.
    pub threads: Option<usize>,
}

/// Reads the cost of the process, no more often than it is worth reading.
///
/// The share of the processor is what was used between two readings, so it needs
/// two of them: the first reading answers nothing and every one after it answers
/// the time since the one before.
#[derive(Debug)]
pub struct Meter {
    interval: Duration,
    taken: Option<Instant>,
    used: Option<Duration>,
    sample: Sample,
    threads: Vec<(String, usize)>,
    drew: VecDeque<Instant>,
    last_frame: Option<Duration>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

impl Meter {
    /// A meter that has read nothing yet.
    pub fn new() -> Self {
        Self {
            interval: Duration::from_millis(500),
            taken: None,
            used: None,
            sample: Sample::default(),
            threads: Vec::new(),
            drew: VecDeque::new(),
            last_frame: None,
        }
    }

    /// Writes down that a frame was drawn, and what it took.
    ///
    /// The moments are kept and not counted into a rate here: a rate worked out
    /// from the time one frame took is a rate this program never drew at — a
    /// frame of four milliseconds says two hundred and fifty a second of a
    /// window that drew one and then waited a minute. What is kept is when the
    /// frames happened, and how many of them fall in the last second is the
    /// answer.
    pub fn drew(&mut self, took: Duration) {
        let now = Instant::now();
        self.last_frame = Some(took);
        self.drew.push_back(now);
        while self
            .drew
            .front()
            .is_some_and(|at| now.duration_since(*at) > OVER)
        {
            self.drew.pop_front();
        }
    }

    /// How long the last frame took this program.
    pub fn last_frame(&self) -> Option<Duration> {
        self.last_frame
    }

    /// How many frames were drawn in the last second.
    ///
    /// It is a count and not a guess, so a window that drew nothing answers
    /// nothing rather than a number worked out from a frame that is long over.
    pub fn frames(&self) -> usize {
        let now = Instant::now();
        self.drew
            .iter()
            .filter(|at| now.duration_since(**at) <= OVER)
            .count()
    }

    /// The cost of the process, read again when the last reading is old.
    ///
    /// A window asks this every frame and pays for it twice a second: reading
    /// `/proc` is three small files, and a number that moves with every frame is
    /// a number nobody can read anyway.
    pub fn sample(&mut self) -> Sample {
        let now = Instant::now();
        if let Some(taken) = self.taken
            && now.duration_since(taken) < self.interval
        {
            return self.sample;
        }

        let used = processor_time();
        let processor = match (self.taken, self.used, used) {
            (Some(taken), Some(before), Some(used)) => {
                let span = now.duration_since(taken).as_secs_f32();
                let busy = used.saturating_sub(before).as_secs_f32();
                (span > 0.0).then(|| busy / span)
            }
            _ => None,
        };

        self.sample = Sample {
            processor,
            ..memory()
        };
        self.threads = thread_names();
        self.taken = Some(now);
        self.used = used;
        self.sample
    }

    /// The threads the process is running, by the name each of them carries,
    /// most of a kind first.
    ///
    /// A thread is named by whoever started it — this program names its own, and
    /// so do the graphics driver, the clipboard and the bus of the desktop — so
    /// the list says which part of the program, or of what it stands on, is
    /// holding them. It is read with the rest, twice a second.
    pub fn threads(&self) -> &[(String, usize)] {
        &self.threads
    }
}

/// What the process holds, as the operating system reports it.
///
/// Every number comes out of one read of one file, because they are compared
/// with one another. The peak is the most memory the process ever held, and the
/// kernel answers it as the larger of what it recorded and what the process
/// holds right now — but only within the one call that formats the file. Read
/// from two files, the pair is two moments: memory handed back between them
/// leaves a peak below a resident size that is no longer held, and a window
/// showing both says the process is over its own high water mark.
#[cfg(target_os = "linux")]
fn memory() -> Sample {
    let mut sample = Sample::default();
    let Ok(text) = std::fs::read_to_string("/proc/self/status") else {
        return sample;
    };

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmSize:") {
            sample.virtual_size = kibibytes(rest);
        } else if let Some(rest) = line.strip_prefix("VmRSS:") {
            sample.resident = kibibytes(rest);
        } else if let Some(rest) = line.strip_prefix("VmHWM:") {
            sample.peak = kibibytes(rest);
        } else if let Some(rest) = line.strip_prefix("Threads:") {
            sample.threads = rest.trim().parse().ok();
        }
    }
    sample
}

/// Nothing, on a platform that does not report it.
#[cfg(not(target_os = "linux"))]
fn memory() -> Sample {
    Sample::default()
}

/// How long the process has had a processor to itself, all its threads counted.
///
/// The name of the program stands in that line in brackets and may hold
/// brackets and blanks of its own, so the fields are counted from the last
/// closing bracket and not from the beginning of the line.
#[cfg(target_os = "linux")]
fn processor_time() -> Option<Duration> {
    const USER_FIELD: usize = 11;
    const SYSTEM_FIELD: usize = 12;

    let text = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = text.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ticks: u64 = fields.get(USER_FIELD)?.parse::<u64>().ok()?
        + fields.get(SYSTEM_FIELD)?.parse::<u64>().ok()?;
    Some(Duration::from_secs_f64(ticks as f64 / TICKS_PER_SECOND))
}

/// Ticks of the clock the kernel counts processor time in.
///
/// It is a build time constant of the kernel and a hundred everywhere this
/// program runs; a machine that says otherwise would make the share of the
/// processor wrong by that factor and nothing else.
#[cfg(target_os = "linux")]
const TICKS_PER_SECOND: f64 = 100.0;

/// Nothing, on a platform that does not report it.
#[cfg(not(target_os = "linux"))]
fn processor_time() -> Option<Duration> {
    None
}

/// The name of every thread of this process, counted per name.
///
/// A name is what the kernel keeps, which is fifteen characters and no more, so
/// two threads of one worker are one line with a count beside it.
#[cfg(target_os = "linux")]
fn thread_names() -> Vec<(String, usize)> {
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };

    let mut counted: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for task in tasks.flatten() {
        let Ok(name) = std::fs::read_to_string(task.path().join("comm")) else {
            continue;
        };
        *counted.entry(name.trim().to_string()).or_default() += 1;
    }

    let mut threads: Vec<(String, usize)> = counted.into_iter().collect();
    threads.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    threads
}

/// Nothing, on a platform that does not report it.
#[cfg(not(target_os = "linux"))]
fn thread_names() -> Vec<(String, usize)> {
    Vec::new()
}

/// Bytes of a line that ends in `kB`.
#[cfg(target_os = "linux")]
fn kibibytes(field: &str) -> Option<u64> {
    field
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kibibytes| kibibytes * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rate is the frames that happened, not a rate worked out from how
    /// long one of them took: a frame of four milliseconds says two hundred and
    /// fifty a second of a window that drew one and then waited.
    #[test]
    fn the_frames_of_the_last_second_are_counted_and_not_guessed() {
        let mut meter = Meter::new();
        assert_eq!(meter.frames(), 0, "nothing was drawn yet");
        assert_eq!(meter.last_frame(), None);

        for took in [2, 4, 6] {
            meter.drew(Duration::from_millis(took));
        }
        assert_eq!(meter.frames(), 3);
        assert_eq!(meter.last_frame(), Some(Duration::from_millis(6)));

        std::thread::sleep(OVER + Duration::from_millis(50));
        assert_eq!(
            meter.frames(),
            0,
            "a frame older than the second it is counted over is not in it"
        );
        assert_eq!(
            meter.last_frame(),
            Some(Duration::from_millis(6)),
            "what the last one took is still what it took"
        );
    }

    /// The machine this runs on answers, and answers something plausible: a
    /// process holds memory, never more than it ever held, and has at least one
    /// thread. The peak holds against the resident size because both are read
    /// out of the same moment; read out of two, memory handed back between them
    /// would put the process over its own high water mark.
    ///
    /// The count and the names are not compared with each other: a suite runs its
    /// tests on threads of its own, and how many there are at one instant is not
    /// how many there are at the next.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_cost_of_this_process_is_read_from_the_system() {
        let mut meter = Meter::new();

        let first = meter.sample();
        assert!(first.resident.is_some_and(|bytes| bytes > 0));
        assert!(
            first
                .peak
                .is_some_and(|bytes| bytes >= first.resident.unwrap())
        );
        assert!(first.threads.is_some_and(|threads| threads >= 1));
        assert!(!meter.threads().is_empty(), "and says what they are called");
        assert!(
            meter
                .threads()
                .iter()
                .all(|(name, count)| { !name.is_empty() && *count >= 1 }),
            "each name is a name and stands for at least one thread"
        );
        assert_eq!(first.processor, None, "one reading says nothing about time");
    }

    /// A reading is kept for as long as the interval says, so a window asking
    /// every frame reads the files twice a second and not sixty times.
    #[test]
    fn a_reading_is_kept_until_it_is_old() {
        let mut meter = Meter::new();

        let first = meter.sample();
        let again = meter.sample();

        assert_eq!(first, again);
        assert!(meter.interval >= Duration::from_millis(100));
    }
}
