//! Running a program of this machine where the device stands.
//!
//! One question goes to the device — where it stands — and the answer is put
//! into the arguments of a program started here. Nothing else of the protocol
//! is used, and the probe that opens a transfer is not sent: it asks for
//! `base64`, `tail`, `head` and `wc`, and a device that has none of them still
//! knows its own directory.
//!
//! The line stays the standard input and the standard output of this process,
//! the way it is for every other command, so the program that is started gets
//! neither: its own standard input is closed and everything it says on either
//! channel is written to the standard error, where the user reads it. A
//! program that would ask a question — a password prompt — therefore fails
//! instead of waiting for an answer nobody can give it.

use sh_xfer::{Mode, Reader, Session};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

/// What stands in an argument for the directory the device is in.
pub const DIRECTORY_PLACEHOLDER: &str = "{}";

/// Asks the device where it stands.
///
/// An answer of nothing at all is a device that did not understand the
/// question, and running the program with an empty directory in its arguments
/// would put the file somewhere nobody asked for.
pub fn working_directory(timeout: u64, mode: Mode) -> sh_xfer::Result<String> {
    let reader = Reader::new(std::io::stdin(), Duration::from_secs(timeout));
    let mut session = Session::new(reader, std::io::stdout(), mode);
    let directory = session.pwd()?;

    if directory.trim().is_empty() {
        return Err(sh_xfer::ShXferError::NoWorkingDirectory);
    }
    Ok(directory.trim().to_string())
}

/// Every argument with the placeholder replaced by the directory.
///
/// It is replaced wherever it stands and not only in an argument that is
/// nothing else, so `user@host:{}` becomes a destination of its own.
pub fn expand(arguments: &[String], directory: &str) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| argument.replace(DIRECTORY_PLACEHOLDER, directory))
        .collect()
}

/// Runs the program and answers with the code it ended on.
///
/// `None` is a program that was ended by a signal and never had a code of its
/// own.
pub fn run(program: &str, arguments: &[String]) -> std::io::Result<Option<i32>> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let out = child
        .stdout
        .take()
        .map(|pipe| to_stderr("sh-xfer-out", pipe));
    let errors = child
        .stderr
        .take()
        .map(|pipe| to_stderr("sh-xfer-err", pipe));

    let status = child.wait()?;
    for thread in [out, errors].into_iter().flatten() {
        let _ = thread.join();
    }
    Ok(status.code())
}

/// Copies one channel of the program to the standard error as it arrives.
///
/// Both of its channels land there, because the standard output of this
/// process is the line: a byte of the program written to it would be read by
/// the device as a command.
fn to_stderr(name: &str, mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let mut buffer = vec![0u8; 8 * 1024];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let mut out = std::io::stderr().lock();
                        if out.write_all(&buffer[..count]).is_err() || out.flush().is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .expect("a thread that copies a pipe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_is_replaced_wherever_it_stands() {
        let arguments = vec![
            "-v".to_string(),
            "/tmp/one".to_string(),
            "root@host:{}".to_string(),
            "{}/{}".to_string(),
        ];

        assert_eq!(
            expand(&arguments, "/usr/bin"),
            vec![
                "-v".to_string(),
                "/tmp/one".to_string(),
                "root@host:/usr/bin".to_string(),
                "/usr/bin//usr/bin".to_string(),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_code_of_the_program_comes_back() {
        assert_eq!(
            run("sh", &["-c".to_string(), "exit 7".to_string()]).unwrap(),
            Some(7)
        );
        assert_eq!(
            run("sh", &["-c".to_string(), "true".to_string()]).unwrap(),
            Some(0)
        );
    }

    #[cfg(unix)]
    #[test]
    fn what_the_program_says_never_reaches_the_line() {
        let out = run(
            "sh",
            &["-c".to_string(), "echo out; echo err 1>&2".to_string()],
        );

        assert_eq!(out.unwrap(), Some(0));
    }
}
