//! The way out of the program.
//!
//! The window goes off the screen first. What holds a port, a program or a
//! thread is let go of in a thread of its own, and the process ends when that
//! is done or when [`LIMIT`] is up. What is left the system closes:
//! descriptors, sockets, ports, and on Windows the programs of a job object.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Longest the way out takes from the moment the window closes.
pub const LIMIT: Duration = Duration::from_millis(300);

/// The thread that lets go: when it started, and its word that it is done.
static LEAVING: Mutex<Option<(Instant, Receiver<()>)>> = Mutex::new(None);

/// Lets go of `parts` in a thread of its own, and returns at once.
pub fn let_go(parts: impl Send + 'static) {
    let started = Instant::now();
    let (done, finished) = channel();
    let spawned = std::thread::Builder::new()
        .name("zyterm-exit".to_string())
        .spawn(move || {
            drop(parts);
            let _ = done.send(());
        });
    if let Err(error) = spawned {
        log::warn!("the thread of the exit did not start: {error}");
        return;
    }
    *LEAVING.lock().unwrap_or_else(|error| error.into_inner()) = Some((started, finished));
}

/// Waits until all is let go of or the limit is up, and says which came first.
pub fn settle() -> bool {
    let Some((started, finished)) = LEAVING
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    else {
        return true;
    };
    let left = LIMIT.saturating_sub(started.elapsed());
    finished.recv_timeout(left).is_ok()
}

/// Ends the process with `code` once [`settle`] has answered.
pub fn finish(code: i32) -> ! {
    if !settle() {
        log::warn!("not all was let go of in {LIMIT:?}; the system closes the rest");
    }
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Session, start_fixture};
    use zyt_script::Targets;

    /// The children of this process that run `program`.
    fn children_running(program: &str) -> Vec<u32> {
        let own = std::process::id().to_string();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| {
                let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                    return false;
                };
                let Some((name, rest)) = stat.split_once(") ") else {
                    return false;
                };
                let mut fields = rest.split_whitespace();
                let state = fields.next();
                let parent = fields.next();
                name.ends_with(&format!("({program}"))
                    && parent == Some(own.as_str())
                    && state != Some("Z")
            })
            .collect()
    }

    /// A busy console and a script with a program that ignores the ask to
    /// stop are let go of inside the limit, and leave nothing running.
    #[cfg(unix)]
    #[test]
    fn a_busy_session_is_let_go_of_inside_the_limit() {
        let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
        session
            .connect_console(&crate::consoles::Console {
                name: "busy".to_string(),
                program: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "trap '' HUP TERM; while :; do echo x; done".to_string(),
                ],
                ..crate::consoles::default_console()
            })
            .expect("the local console starts");
        start_fixture(&mut session, "leaves-a-program", Targets::none())
            .expect("the script starts");
        let deadline = Instant::now() + Duration::from_secs(5);
        while children_running("sleep").is_empty() {
            assert!(Instant::now() < deadline, "the program of the script runs");
            std::thread::sleep(Duration::from_millis(10));
        }

        let started = Instant::now();
        let_go(session.leave());
        let handed = started.elapsed();

        assert!(
            handed < Duration::from_millis(20),
            "the window waited {handed:?}"
        );
        assert!(settle(), "all is let go of inside {LIMIT:?}");
        let deadline = Instant::now() + Duration::from_secs(1);
        while !children_running("sleep").is_empty() {
            assert!(
                Instant::now() < deadline,
                "the program of the script is gone"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
