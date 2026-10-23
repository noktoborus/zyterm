//! The shipped telnet script, run by the runner with `cat` on the line: what
//! a client types comes back through the device, the protocol taken out.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A port of this machine nobody listens on, as far as one can tell.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    listener.local_addr().expect("its address").port()
}

/// The runner with the telnet script and `cat` on a pipe, and a client of it.
fn served() -> (std::process::Child, std::net::TcpStream) {
    let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let port = free_port();
    let runner = Command::new(env!("CARGO_BIN_EXE_zyt-script"))
        .arg("--path")
        .arg(&scripts)
        .arg("--no-default-paths")
        .args(["run", "telnet", "--direction", "receive"])
        .args(["--answer", "host=127.0.0.1"])
        .args(["--answer", &format!("port={port}")])
        .args(["--line", "pipe", "--command", "cat", "--timeout", "20"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the runner starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    let client = loop {
        if let Ok(client) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            break client;
        }
        assert!(Instant::now() < deadline, "the script listens");
        std::thread::sleep(Duration::from_millis(20));
    };
    client
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a timeout");
    (runner, client)
}

/// Reads until `wanted` was heard, or fails with what was.
fn hear(client: &mut std::net::TcpStream, wanted: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut heard = Vec::new();
    let mut buffer = [0u8; 256];
    while !heard.windows(wanted.len()).any(|part| part == wanted) {
        assert!(Instant::now() < deadline, "heard {heard:?}, not {wanted:?}");
        if let Ok(count) = client.read(&mut buffer) {
            heard.extend_from_slice(&buffer[..count]);
        }
    }
    heard
}

/// Commands of the protocol cut at every byte are taken out of what goes to
/// the device; a 255 comes back doubled; RFC 2217 is refused on a line that
/// is no serial port, and so is an option nobody offered.
#[test]
fn the_protocol_is_read_cut_at_every_place() {
    let (mut runner, mut client) = served();
    hear(&mut client, &[255, 253, 31]);

    let sent: &[u8] = &[
        b'a', 255, 251, 44, 255, 250, 31, 0, 80, 0, 24, 255, 240, b'b', 255, 255, b'c', 13, 0,
        b'd', 255, 251, 99,
    ];
    for byte in sent {
        client.write_all(&[*byte]).expect("the client sends");
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut heard = hear(&mut client, &[255, 254, 99]);
    for refused in [[255, 254, 44], [255, 254, 99]] {
        let at = heard.windows(3).position(|part| part == refused);
        let at = at.unwrap_or_else(|| panic!("{refused:?} in {heard:?}"));
        heard.drain(at..at + 3);
    }
    while !heard.ends_with(b"ab\xff\xffc\rd") {
        heard.extend(hear(&mut client, b"d"));
    }

    let _ = runner.kill();
    let _ = runner.wait();
}
