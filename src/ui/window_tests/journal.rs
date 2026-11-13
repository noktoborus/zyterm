//! The journal of the line: what a device says and what is typed go into it,
//! a chunk a line, and a journal that cannot be opened is said and nothing
//! else changes.

use rust_i18n::t;

use super::{Window, window};

/// A directory of its own for the journals of one test.
fn directory() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("zyterm-journals-{}-{count}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a directory for the journals");
    directory
}

fn journal_on(window: &mut Window, directory: &std::path::Path) {
    let app = window.harness.state_mut();
    app.settings.debug.journal_dir = directory.to_path_buf();
    app.settings.debug.journal = true;
    app.apply_journal(false);
}

fn journal_off(window: &mut Window) {
    let app = window.harness.state_mut();
    app.settings.debug.journal = false;
    app.apply_journal(false);
}

/// The one journal of the line in `directory`, as text.
fn the_journal(directory: &std::path::Path) -> String {
    let files: Vec<_> = std::fs::read_dir(directory)
        .expect("the directory reads")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let name = files[0]
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    assert!(name.ends_with("-line.log"), "{name}");
    std::fs::read_to_string(&files[0]).expect("the journal reads")
}

#[test]
fn what_came_and_what_was_typed_go_into_the_journal() {
    let directory = directory();
    let mut window = window();
    journal_on(&mut window, &directory);
    assert!(window.harness.state().session.journal().is_some());

    window.device_says(b"login: ");
    window.step_until("the prompt", |window| {
        window.harness.state().session.counters.bytes_in >= 7
    });
    window
        .harness
        .input_mut()
        .events
        .push(egui::Event::Text("root".to_string()));
    window.wait_written("root");
    journal_off(&mut window);

    let text = the_journal(&directory);
    assert!(text.starts_with("# zyterm "), "{text}");
    assert!(text.contains("\n# source: serial"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with("< ") && line.ends_with(" screen login: ")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("> ") && line.ends_with(" screen root")),
        "{text}"
    );
}

#[test]
fn a_journal_that_cannot_be_opened_is_said_and_the_line_goes_on() {
    let directory = directory();
    let not_a_directory = directory.join("a file");
    std::fs::write(&not_a_directory, b"").expect("a file in the way");
    let mut window = window();
    journal_on(&mut window, &not_a_directory);

    let app = window.harness.state();
    assert!(app.session.journal().is_none(), "no journal");
    let said = app.last_notice.clone().unwrap_or_default();
    assert!(said.contains(&*t!("error.journal")), "{said}");

    window.device_says(b"still here");
    window.step_until("the line going on", |window| {
        window.harness.state().session.counters.bytes_in >= 10
    });
}

/// The one journal of messages in `directory`, as text.
fn the_messages(directory: &std::path::Path) -> String {
    let file = std::fs::read_dir(directory)
        .expect("the directory reads")
        .map(|entry| entry.expect("an entry").path())
        .find(|path| path.to_string_lossy().ends_with("-window.log"))
        .expect("a journal of messages");
    std::fs::read_to_string(file).expect("the journal reads")
}

#[test]
fn the_messages_of_the_libraries_go_without_those_of_zyterm() {
    crate::journal::install_logger();
    let directory = directory();
    let mut window = window();
    {
        let app = window.harness.state_mut();
        app.settings.debug.journal_dir = directory.clone();
        app.settings.debug.messages = crate::config::MessageLevel::Off;
        app.settings.debug.messages_others = crate::config::MessageLevel::Info;
        app.apply_messages(false);
    }
    assert!(
        crate::journal::messages().is_some(),
        "a journal of messages"
    );

    log::info!(target: "winit::journal_test", "a library says 7f3a");
    log::warn!(target: "zyterm::journal_test", "zyterm says 7f3b");
    {
        let app = window.harness.state_mut();
        app.settings.debug.messages_others = crate::config::MessageLevel::Off;
        app.apply_messages(false);
    }
    assert!(crate::journal::messages().is_none(), "closed with both off");

    let text = the_messages(&directory);
    assert!(text.starts_with("# zyterm "), "{text}");
    assert!(
        text.contains(" INFO winit::journal_test a library says 7f3a"),
        "{text}"
    );
    assert!(!text.contains("7f3b"), "nothing of zyterm: {text}");
}
