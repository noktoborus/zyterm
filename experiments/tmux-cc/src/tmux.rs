//! One tmux in control mode on a server of its own, and every byte both ways.

use std::process::Command;
use std::time::{Duration, Instant};

use zyt_pty::{PtyConfig, PtySession};

/// Which way a chunk went.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Written to tmux.
    Sent,
    /// Read from tmux.
    Said,
}

/// Bytes as they went, in the pieces they went in.
pub struct Chunk {
    pub way: Way,
    pub bytes: Vec<u8>,
}

/// A tmux client in control mode, started on a server nobody else uses.
///
/// The server is named after this process and killed when the value goes, so a
/// run that fails half way leaves no server behind, and no server of the user
/// is ever touched.
pub struct Tmux {
    pty: PtySession,
    binary: String,
    socket: String,
    /// Everything that went, both ways, in order.
    pub log: Vec<Chunk>,
    /// Everything tmux said, joined.
    said: Vec<u8>,
    spare: Vec<u8>,
}

/// How long a wait for something tmux is to say lasts before it is a failure.
pub const PATIENCE: Duration = Duration::from_secs(8);

impl Tmux {
    /// Starts `binary -L <socket> -f /dev/null -CC new -s zyt`, 80x24.
    ///
    /// The shell of the pane is `sh` with a prompt of `$ `, so a recording
    /// carries no host name and no directory of the machine it was made on.
    pub fn start(binary: &str, socket: &str) -> Result<Self, String> {
        let config = PtyConfig {
            program: Some(binary.to_string()),
            args: ["-L", socket, "-f", "/dev/null", "-CC", "new", "-s", "zyt"]
                .iter()
                .map(|word| word.to_string())
                .collect(),
            columns: 80,
            rows: 24,
            cell: (8, 16),
            environment: vec![
                ("SHELL".into(), "/bin/sh".into()),
                ("PS1".into(), "$ ".into()),
                ("ENV".into(), String::new()),
                ("TMUX".into(), String::new()),
            ],
        };
        let pty = PtySession::spawn(&config, None).map_err(|error| format!("{binary}: {error}"))?;
        let mut tmux = Self {
            pty,
            binary: binary.to_string(),
            socket: socket.to_string(),
            log: Vec::new(),
            said: Vec::new(),
            spare: Vec::new(),
        };
        if !tmux.until(b"%session-changed", PATIENCE) {
            return Err(format!("{binary}: control mode did not start"));
        }
        Ok(tmux)
    }

    /// The binary this runs.
    pub fn binary(&self) -> &str {
        &self.binary
    }

    /// The name of the server.
    pub fn socket(&self) -> &str {
        &self.socket
    }

    /// Writes one command line, the newline added.
    pub fn send(&mut self, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        self.pty.write(&bytes);
        self.log.push(Chunk {
            way: Way::Sent,
            bytes,
        });
    }

    /// `send-keys -t <pane> -H`, every byte as hex.
    pub fn type_bytes(&mut self, pane: &str, bytes: &[u8]) {
        let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        self.send(&format!("send-keys -t {pane} -H {}", hex.join(" ")));
    }

    /// Stops reading the client for a while, so tmux falls behind.
    pub fn hold(&mut self, time: Duration) {
        self.pty.set_read_hold(true);
        std::thread::sleep(time);
        self.pty.set_read_hold(false);
    }

    /// Reads for as long as `time`.
    pub fn pump(&mut self, time: Duration) {
        let end = Instant::now() + time;
        while Instant::now() < end {
            self.take();
            std::thread::sleep(Duration::from_millis(10));
        }
        self.take();
    }

    /// Reads until `needle` stands in what tmux said since `from`, or `time`
    /// went by.
    pub fn until_from(&mut self, from: usize, needle: &[u8], time: Duration) -> bool {
        let end = Instant::now() + time;
        loop {
            self.take();
            if find(&self.said[from.min(self.said.len())..], needle).is_some() {
                return true;
            }
            if Instant::now() >= end || !self.pty.is_running() && self.pty.read_buffer().0 == 0 {
                self.take();
                return find(&self.said[from.min(self.said.len())..], needle).is_some();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Reads until `needle` stands anywhere in what tmux said.
    pub fn until(&mut self, needle: &[u8], time: Duration) -> bool {
        self.until_from(0, needle, time)
    }

    /// How much tmux said so far: a mark to read from.
    pub fn mark(&self) -> usize {
        self.said.len()
    }

    /// What tmux said since `from`.
    pub fn said_since(&self, from: usize) -> &[u8] {
        &self.said[from.min(self.said.len())..]
    }

    /// Sends one command line and returns the replies to it, in order: the
    /// lines of each, and whether it failed.
    ///
    /// A line of several commands joined by ` ; ` is answered once per command,
    /// so `count` says how many replies to wait for.
    pub fn ask(&mut self, line: &str, count: usize) -> Vec<(Vec<String>, bool)> {
        let from = self.mark();
        self.send(line);
        let end = Instant::now() + PATIENCE;
        loop {
            self.take();
            let replies = replies(self.said_since(from));
            if replies.len() >= count || Instant::now() >= end {
                return replies.into_iter().take(count).collect();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The one reply of a command of one command, its lines.
    pub fn answer(&mut self, line: &str) -> Vec<String> {
        self.ask(line, 1)
            .into_iter()
            .next()
            .map(|(lines, _)| lines)
            .unwrap_or_default()
    }

    /// Leaves the mode with `detach` and reads what is left.
    pub fn detach(&mut self) {
        let from = self.mark();
        self.send("detach");
        self.until_from(from, b"\x1b\\", PATIENCE);
        self.pump(Duration::from_millis(200));
    }

    fn take(&mut self) {
        self.pty.read_into(&mut self.spare);
        if self.spare.is_empty() {
            return;
        }
        self.said.extend_from_slice(&self.spare);
        self.log.push(Chunk {
            way: Way::Said,
            bytes: std::mem::take(&mut self.spare),
        });
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        for socket in [self.socket.clone(), format!("{}-inner", self.socket)] {
            let _ = Command::new(&self.binary)
                .args(["-L", &socket, "kill-server"])
                .stderr(std::process::Stdio::null())
                .status();
        }
        let _ = self.pty.shutdown();
    }
}

/// The replies to commands of this client (flags `1`) in `bytes`, each its
/// lines and whether it was `%error`.
pub fn replies(bytes: &[u8]) -> Vec<(Vec<String>, bool)> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut open: Option<Vec<String>> = None;
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        let ours = line.split(' ').nth(3) == Some("1");
        if line.starts_with("%begin ") && ours {
            open = Some(Vec::new());
        } else if (line.starts_with("%end ") || line.starts_with("%error ")) && ours {
            if let Some(lines) = open.take() {
                out.push((lines, line.starts_with("%error ")));
            }
        } else if let Some(lines) = open.as_mut() {
            lines.push(line.to_string());
        }
    }
    out
}

/// What `binary -V` says, without the name: `3.3a`.
pub fn version(binary: &str) -> Option<String> {
    let output = Command::new(binary).arg("-V").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .last()
        .map(|word| word.trim_start_matches("tmux-").to_string())
}

/// The binaries of tmux on the path, the plain one first.
pub fn installed() -> Vec<String> {
    ["tmux", "tmux-3.0a", "tmux-3.2a", "tmux-3.4", "tmux-3.8"]
        .iter()
        .filter(|binary| version(binary).is_some())
        .map(|binary| binary.to_string())
        .collect()
}

/// A name for a server of this run.
pub fn socket(tag: &str) -> String {
    format!("zyterm-cc-{}-{tag}", std::process::id())
}

/// The first place `needle` stands in `hay`.
pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.windows(needle.len())
        .position(|window| window == needle)
}

/// A version as numbers to compare: `3.3a` is `(3, 3, 1)`.
pub fn ordered(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.');
    let major = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let rest = parts.next().unwrap_or("0");
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let letter = rest
        .chars()
        .find(char::is_ascii_lowercase)
        .map(|c| c as u32 - 'a' as u32 + 1);
    (major, digits.parse().unwrap_or(0), letter.unwrap_or(0))
}
