//! The programs running beside the line, kept by number.
//!
//! Several of them run at once, because none of them holds the line: each is
//! started, watched, stopped and thrown away by its number, and each keeps the
//! file its output went into for as long as its entry stands.

use crate::detached::{DetachedJob, Outcome};
use crate::error::{Result, XferError};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Number of a job, handed out in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

/// One job of the list, running or finished.
#[derive(Debug, Clone)]
pub struct JobState {
    /// Number of the job.
    pub id: JobId,
    /// What it runs, as the caller named it.
    pub title: String,
    /// File everything it said went into.
    pub log: PathBuf,
    /// When it started.
    pub started: Instant,
    /// When it ended, or nothing while it runs.
    pub ended: Option<Instant>,
    /// How it ended, or nothing while it runs.
    pub outcome: Option<Outcome>,
    /// True once it was asked to stop and before it has.
    pub cancelled: bool,
}

impl JobState {
    /// How long it ran, or has been running.
    pub fn elapsed(&self) -> Duration {
        self.ended
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(self.started)
    }

    /// True while the program is still going.
    pub fn is_running(&self) -> bool {
        self.outcome.is_none()
    }
}

/// A job and everything known about it.
#[derive(Debug)]
struct Job {
    state: JobState,
    job: DetachedJob,
}

impl Drop for Job {
    /// Takes the file of the output with the entry that named it.
    ///
    /// The entry is the only thing that says where that file stands, so a file
    /// left behind is a file nobody can find again.
    fn drop(&mut self) {
        let _ = self.job.cancel();
        if let Err(error) = std::fs::remove_file(&self.state.log)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::debug!("the log of a job stays behind: {error}");
        }
    }
}

/// The jobs of the application, each on a process of its own.
#[derive(Debug, Default)]
pub struct JobRunner {
    next: u64,
    jobs: BTreeMap<JobId, Job>,
}

impl JobRunner {
    /// A runner with nothing running.
    pub fn new() -> Self {
        Self {
            next: 1,
            jobs: BTreeMap::new(),
        }
    }

    /// Starts one command line and answers with its number.
    ///
    /// `title` is what the caller calls the job; `notify` is called when the
    /// program ends, so a caller that sleeps between frames wakes up for it.
    pub fn start(
        &mut self,
        title: &str,
        line: &str,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<JobId> {
        let log = log_path()?;
        let job = DetachedJob::start(line, &log, notify)?;

        let id = JobId(self.next.max(1));
        self.next = id.0 + 1;
        self.jobs.insert(
            id,
            Job {
                state: JobState {
                    id,
                    title: title.to_string(),
                    log,
                    started: Instant::now(),
                    ended: None,
                    outcome: None,
                    cancelled: false,
                },
                job,
            },
        );
        Ok(id)
    }

    /// Asks one job to stop where it is.
    pub fn cancel(&mut self, id: JobId) -> Result<()> {
        match self.jobs.get_mut(&id) {
            Some(job) => {
                job.state.cancelled = true;
                job.job.cancel()
            }
            None => Ok(()),
        }
    }

    /// Asks every job to stop where it is.
    pub fn cancel_all(&mut self) {
        for job in self.jobs.values_mut() {
            job.state.cancelled = true;
            if let Err(error) = job.job.cancel() {
                log::warn!("stopping a job: {error}");
            }
        }
    }

    /// Takes one entry off the list, with the file of its output.
    ///
    /// A job that is still running is stopped first: an entry that is gone is
    /// the only thing that knew about the program behind it.
    pub fn remove(&mut self, id: JobId) {
        self.jobs.remove(&id);
    }

    /// Brings the list up to date and names the jobs that ended since the last
    /// call.
    pub fn poll(&mut self) -> Vec<JobId> {
        let mut ended = Vec::new();
        for job in self.jobs.values_mut() {
            job.state.cancelled |= job.job.is_cancelled();
            if job.state.outcome.is_some() {
                continue;
            }
            if let Some(outcome) = job.job.outcome() {
                job.state.outcome = Some(outcome);
                job.state.ended = Some(Instant::now());
                ended.push(job.state.id);
            }
        }
        ended
    }

    /// Every job, oldest first, running and finished alike.
    pub fn jobs(&self) -> Vec<JobState> {
        self.jobs.values().map(|job| job.state.clone()).collect()
    }

    /// How many of them are still running.
    pub fn running(&self) -> usize {
        self.jobs
            .values()
            .filter(|job| job.state.is_running())
            .count()
    }

    /// True while the list is empty.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

/// A file of its own for the output of one job.
///
/// It is named `.txt` because it is read by whatever the desktop opens text
/// with, and it is made here rather than by the program so the name is free
/// before anything writes to it.
fn log_path() -> Result<PathBuf> {
    let file = tempfile::Builder::new()
        .prefix("zyt-xfer-")
        .suffix(".txt")
        .tempfile()
        .map_err(|source| XferError::Log {
            path: std::env::temp_dir(),
            source,
        })?;
    let (_, path) = file.keep().map_err(|error| XferError::Log {
        path: error.file.path().to_path_buf(),
        source: error.error,
    })?;
    Ok(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn wait_for(runner: &mut JobRunner, id: JobId) -> Outcome {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if runner.poll().contains(&id) {
                return runner
                    .jobs()
                    .into_iter()
                    .find(|job| job.id == id)
                    .and_then(|job| job.outcome)
                    .expect("the job ended");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the job never ended");
    }

    #[test]
    fn what_a_job_says_lands_in_its_file() {
        let mut runner = JobRunner::new();
        let id = runner
            .start("greeting", "echo out; echo err 1>&2", None)
            .expect("the job starts");

        assert_eq!(wait_for(&mut runner, id), Outcome::Done);

        let state = runner
            .jobs()
            .into_iter()
            .find(|job| job.id == id)
            .expect("the entry stands");
        let text = std::fs::read_to_string(&state.log).expect("the file is there");
        assert!(text.contains("out") && text.contains("err"), "{text}");
        assert_eq!(
            state.log.extension().and_then(|part| part.to_str()),
            Some("txt")
        );

        runner.remove(id);
        assert!(runner.is_empty());
        assert!(!state.log.exists(), "the file goes with the entry");
    }

    #[test]
    fn a_failed_job_carries_its_code() {
        let mut runner = JobRunner::new();
        let id = runner.start("failing", "exit 3", None).expect("it starts");

        assert_eq!(wait_for(&mut runner, id), Outcome::Failed(Some(3)));
        assert_eq!(runner.running(), 0, "it is no longer running");
        assert_eq!(runner.jobs().len(), 1, "but its entry stands");
    }

    #[test]
    fn several_jobs_run_at_once_and_one_stops_alone() {
        let mut runner = JobRunner::new();
        let long = runner.start("long", "sleep 30", None).expect("it starts");
        let quick = runner.start("quick", "true", None).expect("it starts");
        assert_eq!(runner.running(), 2);

        assert_eq!(wait_for(&mut runner, quick), Outcome::Done);
        runner.cancel(long).expect("it stops");
        assert_eq!(wait_for(&mut runner, long), Outcome::Cancelled);
    }
}
