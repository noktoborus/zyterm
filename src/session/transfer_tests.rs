//! The session on a fake serial device: what goes over the line, with and
//! without a script.

use super::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zyt_script::Targets;
use zyt_serial::{ControlLines, LineParams, PortBackend, PortHandle, PortId, PortInfo, PortKind};

/// A device that records what the application writes and hands back what
/// the test injects.
#[derive(Default)]
pub(crate) struct Device {
    pub(crate) written: Vec<u8>,
    pub(crate) incoming: Vec<u8>,
    pending_write: usize,
    held_break: bool,
    unplugged: bool,
}

#[derive(Clone)]
struct FakeBackend {
    device: Arc<Mutex<Device>>,
}

impl PortBackend for FakeBackend {
    fn list(&self) -> zyt_serial::Result<Vec<PortInfo>> {
        Ok(vec![PortInfo {
            path: "/dev/fake".to_string(),
            kind: PortKind::Builtin,
            usb: None,
            accessible: true,
        }])
    }

    fn open(&self, _path: &str, _params: &LineParams) -> zyt_serial::Result<Box<dyn PortHandle>> {
        Ok(Box::new(FakeHandle {
            device: self.device.clone(),
        }))
    }
}

struct FakeHandle {
    device: Arc<Mutex<Device>>,
}

impl PortHandle for FakeHandle {
    fn read(&mut self, buf: &mut [u8]) -> zyt_serial::Result<usize> {
        std::thread::sleep(Duration::from_millis(5));
        let mut device = self.device.lock().unwrap();
        if std::mem::take(&mut device.unplugged) {
            return Err(zyt_serial::PortError::Disconnected);
        }
        if device.incoming.is_empty() {
            return Ok(0);
        }
        let count = device.incoming.len().min(buf.len());
        buf[..count].copy_from_slice(&device.incoming[..count]);
        device.incoming.drain(..count);
        Ok(count)
    }

    fn write_some(&mut self, data: &[u8]) -> zyt_serial::Result<usize> {
        self.device.lock().unwrap().written.extend_from_slice(data);
        Ok(data.len())
    }

    fn lines(&mut self) -> zyt_serial::Result<ControlLines> {
        Ok(ControlLines::default())
    }

    fn set_rts(&mut self, _level: bool) -> zyt_serial::Result<()> {
        Ok(())
    }

    fn set_dtr(&mut self, _level: bool) -> zyt_serial::Result<()> {
        Ok(())
    }

    fn discard_output(&mut self) -> zyt_serial::Result<()> {
        self.device.lock().unwrap().pending_write = 0;
        Ok(())
    }

    fn line_changes(&mut self) -> Option<zyt_serial::LineEdges> {
        None
    }

    fn set_break(&mut self, held: bool) -> zyt_serial::Result<()> {
        self.device.lock().unwrap().held_break = held;
        Ok(())
    }

    fn set_params(&mut self, _params: &LineParams) -> zyt_serial::Result<()> {
        Ok(())
    }

    fn pending_write(&mut self) -> zyt_serial::Result<usize> {
        Ok(self.device.lock().unwrap().pending_write)
    }

    fn pending_read(&mut self) -> zyt_serial::Result<usize> {
        Ok(self.device.lock().unwrap().incoming.len())
    }
}

pub(crate) fn connected() -> (Session, Arc<Mutex<Device>>) {
    let device = Arc::new(Mutex::new(Device::default()));
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session
        .connect_serial_with(
            PortId::Path("/dev/fake".to_string()),
            "/dev/fake".to_string(),
            LineParams::default(),
            zyt_serial::LineHolds::default(),
            Box::new(FakeBackend {
                device: device.clone(),
            }),
        )
        .expect("the fake port opens");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && session.state() != ConnectionState::Connected {
        session.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(session.state(), ConnectionState::Connected);
    (session, device)
}

/// What the device says reaches the terminal with no frame drawn: the screen
/// reads it in its thread, and the window that takes nothing holds nothing
/// back. None of it is thrown away.
#[test]
fn a_source_is_read_with_no_frame() {
    let (mut session, device) = connected();
    session.set_read_buffer(Some(4096));
    let before = {
        session.settle();
        session.counters.bytes_in
    };

    device.lock().unwrap().incoming = vec![b'x'; 64 * 1024];

    assert!(
        waited_for(|| device.lock().unwrap().incoming.is_empty()),
        "the screen read all of it while the window took nothing"
    );
    assert!(
        pump_until(&mut session, |session| session.counters.bytes_in - before
            >= 64 * 1024),
        "and none of it was lost: {} of it came",
        session.counters.bytes_in - before
    );
    assert_eq!(
        session.counters.bytes_in - before,
        64 * 1024,
        "and nothing more"
    );
}

/// A selection stands, and what the device says waits: the page under it
/// does not move, so the selection goes on saying what it said.
///
/// What waited reaches the page the moment the selection goes, and none of
/// it is thrown away.
#[test]
fn a_standing_selection_keeps_the_page_still() {
    let (mut session, device) = connected();

    session.test_terminal().feed(b"picked out\r\n");
    let _ = session
        .test_terminal()
        .selection_start(zyt_term::SelectionKind::Lines, 0, 0);
    let picked = session.test_terminal().selected_text();
    assert!(
        picked.as_deref().unwrap_or_default().contains("picked out"),
        "the line is the selection: {picked:?}"
    );

    device.lock().unwrap().incoming = b"arrived\r\n".to_vec();
    for _ in 0..20 {
        session.pump();
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(
        session.held_for_selection(),
        "the selection holds the source"
    );
    assert!(
        !terminal_text(&mut session).contains("arrived"),
        "the page stood still while the selection did"
    );
    assert_eq!(
        session.test_terminal().selected_text(),
        picked,
        "and the selection says what it said"
    );

    session.test_terminal().selection_clear();
    assert!(
        pump_until(&mut session, |session| terminal_text(session)
            .contains("arrived")),
        "what waited reaches the page the moment the selection goes"
    );
}

/// A picture on the wire reaches the page as a picture: the cells it takes
/// name it, and the line the window writes stands in them.
#[test]
fn a_picture_on_the_wire_reaches_the_page() {
    let (mut session, device) = connected();
    session.test_terminal().set_cell_size(8, 17);

    device.lock().unwrap().incoming = b"a \x1bPq\"1;1;64;36#0;2;100;0;0#0~~~~$-\x1b\\ b".to_vec();

    assert!(
        pump_until(&mut session, |session| terminal_text(session)
            .contains("<sixel")),
        "the picture reaches the page"
    );

    let content = session.test_terminal().content();
    let id = content
        .cell(2, 0)
        .and_then(|cell| cell.image.map(|part| part.id))
        .expect("the cells of the picture name it");
    assert_eq!(
        session
            .test_terminal()
            .picture(id)
            .map(|picture| picture.cells),
        Some((8, 3)),
        "and it took the cells its pixels come to"
    );

    let text = terminal_text(&mut session);
    assert!(
        text.contains("a <sixel 6"),
        "the line of it stands in those cells: {text:?}"
    );
}

/// Waits for something to become true of another thread, within reason.
fn waited_for(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub(super) fn pump_until(
    session: &mut Session,
    mut done: impl FnMut(&mut Session) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        session.pump();
        if done(session) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub(super) fn terminal_text(session: &mut Session) -> String {
    let content = session.test_terminal().content();
    (0..content.rows)
        .map(|row| {
            (0..content.columns)
                .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn what_a_program_of_a_script_writes_reaches_the_device() {
    let (mut session, device) = connected();
    let file = std::env::temp_dir().join(format!("zyterm-send-{}.bin", std::process::id()));
    std::fs::write(&file, b"file-payload").expect("the file is written");

    start_fixture(
        &mut session,
        "sends",
        Targets {
            kind: zyt_script::TargetKind::File,
            paths: vec![file.clone()],
        },
    )
    .expect("the script starts");

    assert!(pump_until(&mut session, |_| {
        let written = device.lock().unwrap().written.clone();
        String::from_utf8_lossy(&written).contains("file-payload")
    }));

    session.cancel_transfer().expect("the script stops");
    let _ = std::fs::remove_file(file);
}

#[test]
fn what_the_device_says_reaches_the_script_and_not_the_terminal() {
    let (mut session, device) = connected();

    start_fixture(&mut session, "mirrors", Targets::none()).expect("the script starts");

    device
        .lock()
        .unwrap()
        .incoming
        .extend_from_slice(b"from-the-device");

    assert!(pump_until(&mut session, |_| {
        let written = device.lock().unwrap().written.clone();
        String::from_utf8_lossy(&written).contains("from-the-device")
    }));

    assert!(
        !terminal_text(&mut session).contains("from-the-device"),
        "the terminal is not fed while a script holds the line"
    );
    session.cancel_transfer().expect("the script stops");
}

#[test]
fn what_a_script_types_reaches_the_device_as_a_person_would_type_it() {
    let (mut session, device) = connected();

    start_fixture(&mut session, "types", Targets::none()).expect("the script starts");

    assert!(pump_until(&mut session, |_| {
        let written = device.lock().unwrap().written.clone();
        String::from_utf8_lossy(&written).contains("cat > 'payload.bin'\r")
    }));

    session.cancel_transfer().expect("the script stops");
}

/// A port that comes back after it was lost says so once, and the first
/// open of a port says nothing.
#[test]
fn a_port_that_comes_back_says_so() {
    let (mut session, device) = connected();
    assert!(
        session.take_told().is_empty(),
        "the first open says nothing"
    );

    device.lock().unwrap().unplugged = true;
    let mut told = Vec::new();
    assert!(pump_until(&mut session, |session| {
        told.extend(session.take_told());
        !told.is_empty() && session.state() == ConnectionState::Connected
    }));

    assert_eq!(
        told,
        vec![Told {
            message: t!("lost.back", port = "/dev/fake").to_string(),
            details: Vec::new(),
        }]
    );
}

#[test]
fn the_run_ends_only_when_the_line_is_free() {
    let (mut session, device) = connected();
    device.lock().unwrap().pending_write = 8192;

    start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");

    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("quick")
    }));
    assert!(session.is_transferring(), "the run waits for the line");

    device.lock().unwrap().pending_write = 0;
    assert!(pump_until(&mut session, |session| !session.is_transferring()));

    // A run that did what it was started for is told by its plate going
    // away, and by the window asking to be looked at.
    assert!(session.take_told().is_empty());
    assert!(session.run_ended);
}

#[test]
fn the_key_a_script_ends_with_goes_out_when_the_line_is_free() {
    let (mut session, device) = connected();
    device.lock().unwrap().pending_write = 4096;

    start_fixture(&mut session, "keyed", Targets::none()).expect("the script starts");

    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("keyed")
    }));
    assert!(
        !device.lock().unwrap().written.contains(&0x03),
        "the key waits for the line"
    );

    device.lock().unwrap().pending_write = 0;
    assert!(pump_until(&mut session, |_| {
        device.lock().unwrap().written.contains(&0x03)
    }));
}

#[test]
fn a_program_can_store_and_read_the_clipboard() {
    let (mut session, device) = connected();
    session.set_clipboard_access(zyt_term::ClipboardAccess::CopyPaste);

    session.test_terminal().feed(b"\x1b]52;c;aGVsbG8=\x07");
    session.pump();
    assert_eq!(session.clipboard_store.take(), Some("hello".to_string()));

    session.test_terminal().feed(b"\x1b]52;c;?\x07");
    session.pump();
    assert!(session.clipboard_requested);

    session.command(zyt_term::Command::AnswerClipboard(Some(
        "from the clipboard".to_string(),
    )));
    session.clipboard_requested = false;

    assert!(pump_until(&mut session, |_| {
        let written = device.lock().unwrap().written.clone();
        let text = String::from_utf8_lossy(&written).to_string();
        text.contains("52;c;") && text.contains("ZnJvbSB0aGUgY2xpcGJvYXJk")
    }));
}

#[test]
fn disconnecting_stops_a_running_script_first() {
    let (mut session, _device) = connected();

    start_fixture(&mut session, "sends", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| session.is_transferring()));

    session.disconnect();

    assert!(!session.is_transferring());
    assert!(!session.has_source());
}

/// Nothing of a run is left once it is over, whichever way it ended.
fn assert_gone(session: &mut Session) {
    assert!(!session.is_transferring(), "no transfer");
    assert_eq!(session.run_number(), None, "no run");
    assert!(session.take_ask().is_none(), "no dialog");
    assert_eq!(session.run_progress(), None, "no progress of the run");
    assert_eq!(session.outside_progress(), None, "none for the desktop");
}

/// What was told of the end of the script called `name`: the start is
/// told under its name too, and is left out.
fn ends_told(session: &mut Session, name: &str) -> Vec<String> {
    session
        .take_told()
        .into_iter()
        .map(|told| told.message)
        .filter(|message| message.starts_with(name))
        .collect()
}

/// Starts `name` and pumps until it said how far it is.
fn reporting(name: &str) -> (Session, Arc<Mutex<Device>>) {
    let (mut session, device) = connected();
    start_fixture(&mut session, name, Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| session
        .run_progress()
        .is_some()));
    (session, device)
}

/// What a script says about its progress is the run's, and never what the
/// status bar shows for a program of the device.
#[test]
fn the_progress_of_a_script_is_not_the_progress_of_the_terminal() {
    let (session, _device) = reporting("reports");

    assert_eq!(
        session.run_progress(),
        Some(zyt_term::ProgressState::Set(40))
    );
    assert_eq!(session.terminal_progress(), None);
    assert_eq!(session.outside_progress(), session.run_progress());
}

/// A script holding the line drops what a program of the device said, and
/// one beside the line leaves it standing: that program still reaches the
/// window. A disconnect drops it too.
#[test]
fn the_progress_of_the_terminal_goes_only_when_the_line_is_taken() {
    let (mut session, _device) = connected();
    session.test_terminal().feed(b"\x1b]9;4;1;70\x07");
    session.pump();
    start_fixture(&mut session, "beside", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| session
        .run_progress()
        .is_some()));
    assert_eq!(
        session.terminal_progress(),
        Some(zyt_term::ProgressState::Set(70))
    );
    assert_eq!(
        session.outside_progress(),
        Some(zyt_term::ProgressState::Set(40)),
        "the desktop is told of the script while it runs"
    );
    session.cancel_transfer().expect("the script stops");
    assert!(pump_until(&mut session, |session| !session.is_transferring()));
    assert_eq!(
        session.outside_progress(),
        Some(zyt_term::ProgressState::Set(70))
    );

    start_fixture(&mut session, "reports", Targets::none()).expect("the script starts");
    assert_eq!(session.terminal_progress(), None, "the line is taken");

    session.disconnect();
    assert!(!session.is_transferring());
    session.test_terminal().feed(b"\x1b]9;4;1;70\x07");
    session.pump();
    assert_eq!(
        session.terminal_progress(),
        Some(zyt_term::ProgressState::Set(70))
    );
    session.disconnect();
    assert_eq!(session.terminal_progress(), None, "a disconnect drops it");
}

/// The speed of a run is the growth of the bytes it says, and it goes
/// with the run.
#[test]
fn a_run_says_how_fast_its_bytes_go() {
    let (mut session, _device) = connected();
    start_fixture(&mut session, "paces", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| {
        session.run_bytes().is_some_and(|(done, _)| done > 0)
    }));

    let speed = session.run_speed().expect("two reports to measure between");
    assert!(speed > 0.0, "{speed}");

    session.cancel_transfer().expect("the script stops");
    assert!(pump_until(&mut session, |session| !session.is_transferring()));
    assert_eq!(session.run_speed(), None);
}

#[test]
fn a_run_that_ended_by_itself_leaves_nothing_behind() {
    let (mut session, _device) = connected();
    start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| !session.is_transferring()));

    assert_gone(&mut session);
    assert!(
        session.take_told().is_empty(),
        "neither the start nor the end of a run that did its work is told"
    );
}

/// The window is asked to look once for every run that ends, and not for
/// a disconnect with nothing running.
#[test]
fn the_end_of_a_run_asks_for_attention_once() {
    let (mut session, _device) = connected();
    session.disconnect();
    assert!(!session.run_ended, "nothing ran");

    let (mut session, _device) = reporting("reports");
    assert!(!session.run_ended, "it runs");
    session.disconnect();
    assert!(session.run_ended);
}

#[test]
fn a_run_cancelled_leaves_nothing_behind_and_says_so_once() {
    let (mut session, _device) = reporting("reports");
    session.take_told();

    session.cancel_transfer().expect("the script stops");
    assert!(pump_until(&mut session, |session| !session.is_transferring()));

    assert_gone(&mut session);
    let ends = ends_told(&mut session, "Says how far it is");
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert!(ends[0].contains("was stopped"), "{ends:?}");
}

/// A script that is over but whose bytes still wait for the line is forced
/// whole: the wait goes, the rest of the run with it, and the bytes stay on
/// their way.
#[test]
fn a_run_waiting_for_the_line_is_forced_whole() {
    let (mut session, device) = connected();
    device.lock().unwrap().pending_write = 8192;
    start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("quick")
    }));
    assert!(session.is_transferring(), "the run waits for the line");

    session.force_transfer(crate::session::ForcedBy::Person);

    assert_gone(&mut session);
    assert_eq!(
        device.lock().unwrap().pending_write,
        8192,
        "what was handed to the line stays on its way"
    );
}

/// A disconnect stops the script, puts its share away and says it ended,
/// once.
#[test]
fn a_run_cut_off_by_a_disconnect_leaves_nothing_behind() {
    let (mut session, _device) = reporting("reports");
    session.take_told();

    session.disconnect();

    assert_gone(&mut session);
    let ends = ends_told(&mut session, "Says how far it is");
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert!(ends[0].contains("was stopped"), "{ends:?}");
}

/// The dialog a script waits on carries the number of its run, and that
/// number is gone with the run.
#[test]
fn a_dialog_belongs_to_the_run_that_asked_it() {
    let (mut session, _device) = connected();
    start_fixture(&mut session, "asks", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| session
        .waiting
        .is_some()));

    let Some(crate::scripts::Waiting::Form(pending)) = session.take_ask() else {
        panic!("the script asks with a form");
    };
    assert_eq!(pending.run(), session.run_number());
    assert!(pending.run().is_some());

    session.cancel_transfer().expect("the script stops");
    assert!(pump_until(&mut session, |session| !session.is_transferring()));
    assert_ne!(pending.run(), session.run_number(), "the run is gone");
    pending.answer(None);
}

#[test]
fn the_marks_of_a_shell_hand_over_the_command_that_was_typed() {
    let (mut session, _device) = connected();

    session
        .test_terminal()
        .feed(b"$ \x1b]133;B\x07ls -la\r\n\x1b]133;C\x07");
    session.pump();

    assert_eq!(session.take_commands(), vec!["ls -la".to_string()]);
    assert!(session.take_commands().is_empty());
}

#[test]
fn a_bell_is_kept_until_somebody_answers_it() {
    let (mut session, _device) = connected();
    assert!(!session.bell);

    session.test_terminal().feed(b"ring\x07");
    session.pump();

    assert!(session.bell, "the bell rang and nobody has answered yet");

    session.bell = false;
    session.pump();
    assert!(!session.bell, "an answered bell does not ring again");
}

/// The bell that ends an operating system command is the terminator of that
/// command and not a bell, so a program that sets the title rings nothing.
#[test]
fn the_bell_that_closes_a_sequence_is_not_a_bell() {
    let (mut session, _device) = connected();

    session.test_terminal().feed(b"\x1b]0;a title\x07");
    session.pump();

    assert!(!session.bell);
}

#[test]
fn a_shell_whose_marks_are_refused_hands_over_nothing() {
    let (mut session, _device) = connected();
    session.marks_enabled = false;

    session
        .test_terminal()
        .feed(b"$ \x1b]133;B\x07ls -la\r\n\x1b]133;C\x07");
    session.pump();

    assert!(session.take_commands().is_empty());
}

#[test]
fn a_long_busy_line_is_remembered() {
    let (mut session, device) = connected();
    assert!(session.last_busy().is_none());

    device.lock().unwrap().pending_write = 4096;
    assert!(pump_until(&mut session, |session| session
        .busy_since
        .is_some()));
    session.busy_since = Some(Instant::now() - Duration::from_secs(3));

    device.lock().unwrap().pending_write = 0;
    assert!(pump_until(&mut session, |session| session
        .last_busy()
        .is_some()));
    assert!(session.last_busy().expect("a span") >= Duration::from_secs(3));
}

#[test]
fn what_a_script_says_lands_in_the_terminal_in_a_frame() {
    let (mut session, _device) = connected();

    start_fixture(&mut session, "says", Targets::none()).expect("the script starts");

    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("│ Bytes Sent: 1024")
    }));

    session.cancel_transfer().expect("the script stops");
}

/// The device says `bytes`.
fn device_says(device: &Arc<Mutex<Device>>, bytes: &[u8]) {
    device.lock().unwrap().incoming.extend_from_slice(bytes);
}

/// A script that asks the host nothing goes on after a Cancel and keeps the
/// line; a Force takes it at once, and what the device says next reaches the
/// terminal.
#[test]
fn a_run_that_does_not_stop_keeps_the_line_until_forced() {
    let (mut session, device) = connected();
    start_fixture(&mut session, "spins", Targets::none()).expect("the script starts");
    assert!(pump_until(&mut session, |session| session.run_holds_line()));

    session.cancel_transfer().expect("the script is asked");
    assert_eq!(session.run_phase(), Some(RunPhase::Asked));
    device_says(&device, b"for the script");
    std::thread::sleep(Duration::from_millis(100));
    session.settle();
    assert!(
        !terminal_text(&mut session).contains("for the script"),
        "the script holds the line after a Cancel"
    );

    session.force_transfer(crate::session::ForcedBy::Person);
    assert!(!session.is_transferring(), "a Force ends the run at once");
    device_says(&device, b"for the terminal");

    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("for the terminal")
    }));
}

/// A script says it ends, and the run stands in that phase until it is over.
#[test]
fn a_script_that_says_it_ends_is_ending() {
    let (mut session, _device) = connected();
    start_fixture(&mut session, "ends-slowly", Targets::none()).expect("the script starts");

    assert!(pump_until(&mut session, |session| {
        session.run_phase() == Some(RunPhase::Ending)
    }));

    session.force_transfer(crate::session::ForcedBy::Person);
    assert!(!session.is_transferring());
}

/// A run sets the port from its own thread; the session shows what it set,
/// and says once, after the run, that the parameters are to be written down.
#[test]
fn a_run_drives_the_port_and_the_session_follows() {
    let (mut session, _device) = connected();
    start_fixture(&mut session, "drives", Targets::none()).expect("the script starts");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && session.is_transferring() {
        session.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!session.is_transferring());
    let told = session.take_told();
    assert!(told.is_empty(), "the run did what it was for: {told:?}");
    assert_eq!(session.params.baud_rate, 9600);
    assert_eq!(session.params.parity, zyt_serial::Parity::Even);
    assert_eq!(session.rts_hold, zyt_serial::LineHold::Up);
    assert!(session.take_params_of_run());
    assert!(!session.take_params_of_run(), "once");
}
