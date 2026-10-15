//! `probe`: the questions the design of control mode rests on, asked of every
//! tmux there is, one fresh server a question.
//!
//! Each answer is one line, short enough to set beside the table of
//! `docs/tmux.md`, *What tmux was seen to do*, and see where a version
//! differs.

use std::time::Duration;

use crate::scenario::at_least;
use crate::tmux::{self, PATIENCE, Tmux, find};

type Question = fn(&str, &str) -> Result<String, String>;

const QUESTIONS: [(&str, Question); 11] = [
    ("opening", opening),
    ("client size", client_size),
    ("terminal answers", terminal_answers),
    ("long command line", long_line),
    ("inside replies", inside_replies),
    ("hidden and other", hidden_and_other),
    ("after select-window", after_select),
    ("alternate capture", alternate_capture),
    ("failed start", failed_start),
    ("pause-after", pause_after),
    ("hyperlink capture", hyperlink),
];

/// `probe [--tmux <binary>]...`.
pub fn run(args: &[&str]) -> Result<(), String> {
    let mut binaries = Vec::new();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match *word {
            "--tmux" => binaries.push(words.next().ok_or("--tmux <binary>")?.to_string()),
            other => return Err(format!("probe: no option {other}")),
        }
    }
    if binaries.is_empty() {
        binaries = tmux::installed();
    }
    for binary in &binaries {
        let version = tmux::version(binary).ok_or(format!("{binary}: no such tmux"))?;
        println!("== {binary} ({version})");
        for (name, question) in QUESTIONS {
            let answer =
                question(binary, &version).unwrap_or_else(|error| format!("FAILED: {error}"));
            println!("  {name:<20} {answer}");
        }
    }
    Ok(())
}

fn start(binary: &str, tag: &str) -> Result<Tmux, String> {
    Tmux::start(binary, &tmux::socket(tag))
}

/// The first line after `ESC P1000p`.
fn opening(binary: &str, _: &str) -> Result<String, String> {
    let tmux = start(binary, "opening")?;
    let said = tmux.said_since(0);
    let after = find(said, b"\x1bP1000p").ok_or("no ESC P1000p")? + 7;
    let line = said[after..]
        .split(|&byte| byte == b'\n')
        .next()
        .unwrap_or_default();
    let line = String::from_utf8_lossy(line).trim_end().to_string();
    let flags = line.split(' ').nth(3).unwrap_or("?");
    Ok(format!(
        "{}, flags {flags}",
        line.split(' ').next().unwrap_or("")
    ))
}

/// Both forms of `refresh-client -C`.
fn client_size(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "size")?;
    let mut seen = Vec::new();
    for (form, wanted) in [("100,30", "100x30"), ("90x20", "90x20")] {
        let from = tmux.mark();
        tmux.send(&format!("refresh-client -C {form}"));
        let took = tmux.until_from(
            from,
            format!(",{wanted},").as_bytes(),
            Duration::from_secs(2),
        );
        seen.push(format!(
            "{form} {}",
            if took { "taken" } else { "not taken" }
        ));
    }
    Ok(seen.join(", "))
}

/// What tmux answers a program: DA1, DA2, `CSI 14t`, `16t`, `18t`, and the
/// window size of the pty.
fn terminal_answers(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "answers")?;
    let me = std::env::current_exe().map_err(|error| error.to_string())?;
    let from = tmux.mark();
    tmux.type_bytes("%0", format!("{} ask\r", me.display()).as_bytes());
    if !tmux.until_from(from, b"ASK winsize", PATIENCE) {
        return Err("ask said nothing".into());
    }
    tmux.pump(Duration::from_millis(200));
    let text = output_of(tmux.said_since(from), "%0");
    let text = String::from_utf8_lossy(&text);
    let answers: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            line.find("ASK ").map(|at| &line[at + 4..])
        })
        .collect();
    Ok(answers.join("; "))
}

/// The longest `send-keys -l` tried, and whether the pane read all of it.
fn long_line(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "long")?;
    const SIZE: usize = 256_000;
    tmux.type_bytes(
        "%0",
        format!("stty raw -echo; head -c {SIZE} | wc -c; stty sane\r").as_bytes(),
    );
    tmux.pump(Duration::from_millis(400));
    let from = tmux.mark();
    tmux.send(&format!("send-keys -t %0 -l '{}'", "a".repeat(SIZE)));
    let read = tmux.until_from(from, SIZE.to_string().as_bytes(), Duration::from_secs(10));
    let refused = find(tmux.said_since(from), b"%error").is_some();
    Ok(format!(
        "{SIZE} bytes in one line: {}, {}",
        if refused { "refused" } else { "taken" },
        if read {
            "all read by the pane"
        } else {
            "not all read"
        }
    ))
}

/// With two panes flooding: any notification inside a reply, any output
/// between the replies of one command line.
fn inside_replies(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "inside")?;
    tmux.send("new-window -d");
    tmux.pump(Duration::from_millis(300));
    let flood = b"i=0; while :; do i=$((i+1)); echo line$i; done\r";
    tmux.type_bytes("%0", flood);
    tmux.type_bytes("%1", flood);
    tmux.pump(Duration::from_millis(500));
    let from = tmux.mark();
    for _ in 0..20 {
        tmux.send("display -p A ; capture-pane -p -t %0 ; display -p B ; capture-pane -p -t %1 ; display -p C");
        tmux.pump(Duration::from_millis(50));
    }
    tmux.pump(Duration::from_millis(800));
    let text = String::from_utf8_lossy(tmux.said_since(from)).into_owned();
    let (mut inside, mut between, mut reply, mut open) = (0, 0, 0usize, false);
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        let ours = line.split(' ').nth(3) == Some("1");
        if line.starts_with("%begin ") && ours {
            open = true;
        } else if (line.starts_with("%end ") || line.starts_with("%error ")) && ours {
            open = false;
            reply += 1;
        } else if open {
            if line.starts_with("%output %") || line.starts_with("%layout-change @") {
                inside += 1;
            }
        } else if line.starts_with("%output ") && reply % 5 != 0 && reply < 100 {
            between += 1;
        }
    }
    tmux.type_bytes("%0", b"\x03");
    tmux.type_bytes("%1", b"\x03");
    Ok(format!(
        "{reply} replies; notifications inside: {inside}; output between the replies of a line: {between}"
    ))
}

/// Output of a pane of a hidden window, and of a pane of another session.
fn hidden_and_other(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "hidden")?;
    tmux.send("new-window -d");
    tmux.send("new-session -d -s other");
    tmux.pump(Duration::from_millis(400));
    let panes = tmux.answer("list-panes -a -F '#{session_name} #{window_active} #{pane_id}'");
    let pane_of = |session: &str, active: &str| {
        panes
            .iter()
            .find(|line| line.starts_with(&format!("{session} {active} ")))
            .and_then(|line| line.split(' ').nth(2))
            .map(str::to_string)
    };
    let hidden = pane_of("zyt", "0").ok_or("no hidden window")?;
    let other = pane_of("other", "1").ok_or("no other session")?;
    let from = tmux.mark();
    tmux.type_bytes(&hidden, b"echo MARK-HIDDEN\r");
    tmux.type_bytes(&other, b"echo MARK-OTHER\r");
    tmux.pump(Duration::from_millis(800));
    let said = tmux.said_since(from);
    let came = |pane: &str, mark: &[u8]| find(&output_of(said, pane), mark).is_some();
    Ok(format!(
        "hidden window {}; other session {}",
        if came(&hidden, b"MARK-HIDDEN") {
            "comes"
        } else {
            "none"
        },
        if came(&other, b"MARK-OTHER") {
            "comes"
        } else {
            "none"
        }
    ))
}

/// What comes first after `select-window`: the replies of its line, or the
/// notification of the switch.
fn after_select(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "select")?;
    tmux.send("new-window -d");
    tmux.pump(Duration::from_millis(300));
    let from = tmux.mark();
    tmux.send("select-window -t @1 ; display -p SELECTED");
    tmux.until_from(from, b"%session-window-changed", Duration::from_secs(2));
    tmux.pump(Duration::from_millis(200));
    let said = tmux.said_since(from);
    let reply = find(said, b"SELECTED");
    let switch = find(said, b"%session-window-changed");
    Ok(match (reply, switch) {
        (Some(reply), Some(switch)) if reply < switch => {
            "replies first, then %session-window-changed".into()
        }
        (Some(_), Some(_)) => "%session-window-changed first, then the replies".into(),
        _ => "incomplete".into(),
    })
}

/// `capture-pane -S` and `-a` while a program is on the alternate screen, and
/// `-a` with none.
fn alternate_capture(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "alternate")?;
    tmux.send("refresh-client -C 40x10");
    tmux.pump(Duration::from_millis(200));
    let none = tmux.ask("capture-pane -p -a -t %0", 1);
    let none = match none.first() {
        Some((lines, true)) => format!("-a with none: %error {}", lines.join(" ")),
        Some((_, false)) => "-a with none: taken".to_string(),
        None => "-a with none: no reply".to_string(),
    };
    let from = tmux.mark();
    tmux.type_bytes(
        "%0",
        b"for i in $(seq 1 40); do echo hist$i; done; printf '\\033[?1049h\\033[HALT-SCREEN'; sleep 30\r",
    );
    tmux.until_from(from, b"ALT-SCREEN", PATIENCE);
    tmux.pump(Duration::from_millis(200));
    // A screen of 10 rows: three lines more than that are the history `-S -3`
    // asked for.
    let with_s = tmux.answer("capture-pane -p -t %0 -S -3");
    let with_a = tmux.answer("capture-pane -p -a -t %0 -S -3");
    let history = |lines: &[String]| lines.len() > 10;
    let alt = |lines: &[String]| lines.iter().any(|line| line.contains("ALT-SCREEN"));
    Ok(format!(
        "{none}; -S on the alternate: history {} alternate {}; -a -S: history {} alternate {}",
        history(&with_s),
        alt(&with_s),
        history(&with_a),
        alt(&with_a)
    ))
}

/// A start that fails: `attach` with no session.
fn failed_start(binary: &str, _: &str) -> Result<String, String> {
    let socket = tmux::socket("failed");
    let output = std::process::Command::new("script")
        .args([
            "-qc",
            &format!("{binary} -L {socket} -f /dev/null -CC attach"),
            "/dev/null",
        ])
        .output()
        .map_err(|error| format!("script: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).replace('\r', "");
    let lines: Vec<&str> = text.lines().take(5).collect();
    Ok(crate::record::escape(lines.join(" | ").as_bytes()))
}

/// `pause-after`, the pause by hand, and the flag taken back.
fn pause_after(binary: &str, version: &str) -> Result<String, String> {
    if !at_least(version, "3.2") {
        let mut tmux = start(binary, "pause")?;
        let reply = tmux.ask("refresh-client -f pause-after=1", 1);
        let refused = reply.first().is_some_and(|(_, failed)| *failed);
        return Ok(format!(
            "before 3.2: refresh-client -f {}",
            if refused { "refused" } else { "taken" }
        ));
    }
    let mut tmux = start(binary, "pause")?;
    tmux.send("refresh-client -f pause-after=1");
    tmux.pump(Duration::from_millis(200));
    // Some 20 KB a second: more than the buffer of the pty holds in the three
    // seconds the client reads nothing, so tmux falls behind; a flood of `yes`
    // would leave megabytes to read back afterwards.
    tmux.type_bytes(
        "%0",
        b"while :; do printf '%0200d\\n' 0; sleep 0.01; done\r",
    );
    tmux.hold(Duration::from_secs(3));
    let from = tmux.mark();
    tmux.pump(Duration::from_millis(800));
    let paused = tmux.until_from(from, b"%pause %0", Duration::from_secs(3));
    let extended = find(tmux.said_since(from), b"%extended-output").is_some();
    let from = tmux.mark();
    tmux.send("refresh-client -A '%0:continue'");
    let continued = tmux.until_from(from, b"%continue %0", Duration::from_secs(2));
    tmux.type_bytes("%0", b"\x03");
    tmux.pump(Duration::from_millis(300));
    let from = tmux.mark();
    tmux.send("refresh-client -A '%0:pause'");
    let by_hand = tmux.until_from(from, b"%pause %0", Duration::from_secs(2));
    let back = tmux.ask("refresh-client -f '!pause-after'", 1);
    let taken_back = back.first().is_some_and(|(_, failed)| !failed);
    tmux.send("refresh-client -A '%0:continue'");
    let from = tmux.mark();
    tmux.type_bytes("%0", b"echo AFTER\r");
    tmux.until_from(from, b"AFTER", Duration::from_secs(2));
    let plain = find(tmux.said_since(from), b"%output %0").is_some();
    Ok(format!(
        "extended-output {extended}; %pause after 3 s unread {paused}; %continue {continued}; pause by hand {by_hand}; !pause-after taken {taken_back}, then %output {plain}"
    ))
}

/// A hyperlink on the screen, and what `capture-pane -e` makes of it.
fn hyperlink(binary: &str, _: &str) -> Result<String, String> {
    let mut tmux = start(binary, "link")?;
    let from = tmux.mark();
    tmux.type_bytes(
        "%0",
        b"clear; printf '\\033]8;;http://x.example\\033\\\\LINK\\033]8;;\\033\\\\\\n'\r",
    );
    tmux.until_from(from, b"LINK", PATIENCE);
    tmux.pump(Duration::from_millis(300));
    let lines = tmux.answer("capture-pane -p -e -t %0");
    let text = lines.join("\n");
    let osc8 = text.contains("\x1b]8;");
    let st = text.contains("\x1b\\");
    Ok(format!(
        "OSC 8 in the reply {osc8}, a raw ESC \\ in it {st}"
    ))
}

/// The bytes of a pane's `%output` and `%extended-output` in `said`, decoded.
fn output_of(said: &[u8], pane: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(said);
    let mut out = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        let rest = if let Some(rest) = line.strip_prefix(&format!("%output {pane} ")) {
            rest
        } else if let Some(rest) = line.strip_prefix(&format!("%extended-output {pane} ")) {
            rest.split_once(" : ").map(|(_, rest)| rest).unwrap_or("")
        } else {
            continue;
        };
        out.extend(octal(rest.as_bytes()));
    }
    out
}

/// `\ooo` to the byte it stands for.
fn octal(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && at + 3 < bytes.len() + 1 && bytes[at + 1..].len() >= 3 {
            let digits = &bytes[at + 1..at + 4];
            if digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let value = digits
                    .iter()
                    .fold(0u32, |value, digit| value * 8 + (digit - b'0') as u32);
                out.push(value as u8);
                at += 4;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    out
}
