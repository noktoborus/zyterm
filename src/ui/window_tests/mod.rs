//! Tests of the window (`RULE test.window`): `App` driven by `egui_kittest`
//! with egui events, what it draws found through the tree of accesskit, and
//! what went to the line read from the fake device of the session tests.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::config::Settings;
use crate::session::TmuxLook;
use crate::session::transfer_tests::{Device, connected};
use zyt_config::ConfigStore;

mod access;
mod tmux;

/// How long a step of a test may wait for the window to show what it waits
/// for: the fake port answers in its own thread.
const BOUND: Duration = Duration::from_secs(5);

/// The window and the device on the other end of its line.
pub struct Window {
    pub harness: Harness<'static, App>,
    pub device: Arc<Mutex<Device>>,
}

/// The window on a fresh store, connected to the fake port.
pub fn window() -> Window {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("zyterm-window-{}-{count}", std::process::id()));
    let store = ConfigStore::with_paths(
        directory.join("config"),
        directory.join("data"),
        directory.join("lock"),
    );
    let (session, device) = connected();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_eframe(move |creation| {
            let mut app = App::new(&creation.egui_ctx, store, Settings::default(), None)
                .expect("the application starts");
            app.session = session;
            app
        });
    harness.run_steps(2);
    Window { harness, device }
}

impl Window {
    /// Bytes from the device to the window.
    pub fn device_says(&self, bytes: &[u8]) {
        self.device
            .lock()
            .unwrap()
            .incoming
            .extend_from_slice(bytes);
    }

    /// What the window wrote to the line since the last call.
    pub fn written(&self) -> String {
        let bytes = std::mem::take(&mut self.device.lock().unwrap().written);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Steps the window until `done`, or panics past `BOUND`.
    pub fn step_until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + BOUND;
        while !done(self) {
            assert!(Instant::now() < deadline, "the window never came to {what}");
            self.harness.step();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Steps the window until it writes a line holding `wanted`; what it wrote
    /// up to there.
    pub fn wait_written(&mut self, wanted: &str) -> String {
        let mut seen = String::new();
        self.step_until(wanted, |window| {
            seen.push_str(&window.written());
            seen.contains(wanted)
        });
        seen
    }

    /// The window in tmux control mode on session `$0 zyt`: the opening pair,
    /// the version, the lists, the snapshot of the pane shown — answered as
    /// tmux answers them.
    pub fn in_tmux(&mut self, windows: &str, panes: &str, screen: &str) {
        self.in_tmux_with(MODES, windows, panes, screen);
    }

    /// The same, the pane described by `modes`, the reply of `display -p`.
    pub fn in_tmux_with(&mut self, modes: &str, windows: &str, panes: &str, screen: &str) {
        self.device_says(b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n");
        self.wait_written("display -p '#{version}'");
        self.device_says(&replies(2, &["3.3a"]));
        let start = self.wait_written("list-panes");
        let commands = start.trim_end().split(" ; ").count();
        let mut answers = vec!["", "$0 zyt", windows, panes];
        answers.resize(commands.max(answers.len()), "");
        self.device_says(&replies(3, &answers));
        self.wait_written("capture-pane");
        self.device_says(&replies(7, &[modes, "", screen]));
        self.step_until("the pane shown", |window| {
            matches!(
                window.harness.state().session.tmux_look(),
                Some(TmuxLook::Shown {
                    window: Some(_),
                    ..
                })
            )
        });
        self.harness.run_steps(2);
    }

    /// Clicks the button of tmux, and answers the lists and the notice the
    /// menu asks for as it opens.
    pub fn open_tmux_menu(&mut self, button: &str, windows: &str, panes: &str) {
        self.harness.get_by_label(button).click();
        self.wait_written("list-panes");
        self.device_says(&replies(100, &["$0 zyt", windows, panes, "3.3a 80x24 1"]));
        self.harness.run_steps(4);
    }

    /// Whether an element with exactly this label stands in the window.
    pub fn shows(&self, label: &str) -> bool {
        self.harness.query_by_label(label).is_some()
    }
}

/// What tmux says in reply to the commands of one line, numbered from
/// `number`.
pub fn replies(number: u32, bodies: &[&str]) -> Vec<u8> {
    let mut text = String::new();
    for (offset, body) in bodies.iter().enumerate() {
        let number = number + offset as u32;
        text.push_str(&format!("%begin 1 {number} 1\r\n"));
        for line in body.lines() {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text.push_str(&format!("%end 1 {number} 1\r\n"));
    }
    text.into_bytes()
}

/// The reply of `display -p` for a pane of 80x24.
pub const MODES: &str = "80 24 0 0 4294967295 4294967295 2 0 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16";
