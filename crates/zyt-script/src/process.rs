//! Starting and stopping a command line of the platform.
//!
//! Both kinds of work a script drives — a program on the line of the
//! terminal and one writing its output to a file — may start a shell, and
//! everything that shell started has to be stoppable, so the two share the
//! way it is done rather than each knowing it.

use crate::error::{Result, ScriptError};
use std::process::Command;
use std::time::Duration;

/// Puts a command in a group of its own, so everything it starts can be
/// stopped together.
pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// Stops every process of the group, not only the shell that started it.
pub fn stop_group(pid: u32) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};

        let Some(group) = Pid::from_raw(pid as i32) else {
            return;
        };
        let _ = kill_process_group(group, Signal::TERM);
        std::thread::sleep(Duration::from_millis(50));
        let _ = kill_process_group(group, Signal::KILL);
    }

    #[cfg(windows)]
    {
        use std::process::Stdio;

        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
}

/// Shell of the platform and the flag that makes it run one command line.
pub fn shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// A shell of the platform set up to run one line, in a group of its own.
pub fn shell_command(line: &str) -> Command {
    let (shell, flag) = shell();
    let mut command = Command::new(shell);
    command.arg(flag).arg(line);
    own_group(&mut command);
    command
}

/// Starts a named thread, or says why it could not.
pub fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(|_| ())
        .map_err(|source| ScriptError::ThreadStart { source })
}
