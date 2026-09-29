//! State machine tests driven by a fake device.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zyt_serial::{
    ControlLines, LineHold, LineParams, PortBackend, PortError, PortEvent, PortHandle, PortId,
    PortInfo, PortKind, PortState, PortSupervisor, Result, Signal, SupervisorConfig, UsbInfo,
};

struct Device {
    present: bool,
    path: String,
    incoming: Vec<u8>,
    written: Vec<u8>,
    fail_next_read: bool,
    rts: bool,
    held_break: bool,
    pending_write: usize,
    /// Most the driver takes in one write, as a slow line would.
    write_chunk: usize,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            present: false,
            path: String::new(),
            incoming: Vec::new(),
            written: Vec::new(),
            fail_next_read: false,
            rts: false,
            held_break: false,
            pending_write: 0,
            write_chunk: usize::MAX,
        }
    }
}

#[derive(Clone)]
struct FakeBackend {
    device: Arc<Mutex<Device>>,
}

impl PortBackend for FakeBackend {
    fn list(&self) -> Result<Vec<PortInfo>> {
        let device = self.device.lock().unwrap();
        if !device.present {
            return Ok(Vec::new());
        }
        Ok(vec![PortInfo {
            path: device.path.clone(),
            kind: PortKind::Usb,
            usb: Some(UsbInfo {
                vid: 0x0403,
                pid: 0x6001,
                serial_number: Some("A1".to_string()),
                manufacturer: None,
                product: None,
            }),
            accessible: true,
        }])
    }

    fn open(&self, path: &str, _params: &LineParams) -> Result<Box<dyn PortHandle>> {
        let device = self.device.lock().unwrap();
        if !device.present {
            return Err(PortError::NotFound {
                path: path.to_string(),
            });
        }
        drop(device);
        self.device.lock().unwrap().rts = true;
        Ok(Box::new(FakeHandle {
            device: self.device.clone(),
        }))
    }
}

struct FakeHandle {
    device: Arc<Mutex<Device>>,
}

impl PortHandle for FakeHandle {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        std::thread::sleep(Duration::from_millis(5));
        let mut device = self.device.lock().unwrap();
        if device.fail_next_read || !device.present {
            device.fail_next_read = false;
            return Err(PortError::Disconnected);
        }
        if device.incoming.is_empty() {
            return Ok(0);
        }
        let count = device.incoming.len().min(buf.len());
        buf[..count].copy_from_slice(&device.incoming[..count]);
        device.incoming.drain(..count);
        Ok(count)
    }

    fn write_some(&mut self, data: &[u8]) -> Result<usize> {
        let mut device = self.device.lock().unwrap();
        if !device.present {
            return Err(PortError::Disconnected);
        }
        let taken = data.len().min(device.write_chunk);
        device.written.extend_from_slice(&data[..taken]);
        Ok(taken)
    }

    fn lines(&mut self) -> Result<ControlLines> {
        let device = self.device.lock().unwrap();
        Ok(ControlLines {
            rts_asked: device.rts,
            rts: Some(device.rts),
            cts: device.present,
            ..ControlLines::default()
        })
    }

    fn set_rts(&mut self, level: bool) -> Result<()> {
        self.device.lock().unwrap().rts = level;
        Ok(())
    }

    fn set_dtr(&mut self, _level: bool) -> Result<()> {
        Ok(())
    }

    fn set_break(&mut self, held: bool) -> Result<()> {
        self.device.lock().unwrap().held_break = held;
        Ok(())
    }

    fn pending_read(&mut self) -> Result<usize> {
        Ok(self.device.lock().unwrap().incoming.len())
    }

    fn set_params(&mut self, _params: &LineParams) -> Result<()> {
        Ok(())
    }

    fn pending_write(&mut self) -> Result<usize> {
        Ok(self.device.lock().unwrap().pending_write)
    }
}

fn target() -> PortId {
    PortId::Usb {
        vid: 0x0403,
        pid: 0x6001,
        serial_number: Some("A1".to_string()),
    }
}

fn config() -> SupervisorConfig {
    let mut config = SupervisorConfig::new(target());
    config.scan_interval = Duration::from_millis(20);
    config.lines_interval = Duration::from_millis(20);
    config
}

fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn opens_when_device_appears_and_reopens_after_loss() {
    let device = Arc::new(Mutex::new(Device {
        present: false,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert_eq!(supervisor.status().state, PortState::Disconnected);

    device.lock().unwrap().present = true;
    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));

    device.lock().unwrap().incoming.extend_from_slice(b"hello");
    let mut buffer = Vec::new();
    assert!(wait_for(|| {
        supervisor.read_into(&mut buffer);
        buffer == b"hello"
    }));

    {
        let mut guard = device.lock().unwrap();
        guard.present = false;
        guard.fail_next_read = true;
    }
    assert!(wait_for(
        || supervisor.status().state == PortState::Disconnected
    ));

    {
        let mut guard = device.lock().unwrap();
        guard.present = true;
        guard.path = "/dev/ttyUSB1".to_string();
    }
    assert!(wait_for(|| {
        supervisor.status().path.as_deref() == Some("/dev/ttyUSB1")
            && supervisor.status().state == PortState::Connected
    }));
}

/// A line held from here is held again on the next open.
///
/// The driver of this fake raises `RTS` when the port opens, the way a real one
/// does. A hold that lasted as long as the connection would leave the line
/// standing up after a device was unplugged and came back, which is the moment
/// somebody holding a board in reset would notice it least.
#[test]
fn a_forced_hold_is_put_back_on_the_line_after_a_reconnect() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));
    assert!(device.lock().unwrap().rts, "opening raises the line");

    supervisor
        .set_rts(LineHold::Down)
        .expect("command accepted");
    assert!(wait_for(|| !device.lock().unwrap().rts));

    {
        let mut guard = device.lock().unwrap();
        guard.present = false;
        guard.fail_next_read = true;
    }
    assert!(wait_for(
        || supervisor.status().state == PortState::Disconnected
    ));
    device.lock().unwrap().present = true;
    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));

    assert!(
        wait_for(|| !device.lock().unwrap().rts),
        "the hold outlives the connection it was asked for"
    );
}

#[test]
fn the_status_carries_what_is_still_waiting_for_the_line() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        pending_write: 4096,
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(|| supervisor.status().pending_output == 4096));

    device.lock().unwrap().pending_write = 0;
    assert!(wait_for(|| supervisor.status().pending_output == 0));
}

#[test]
fn a_driver_that_takes_a_little_at_a_time_loses_nothing() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        write_chunk: 8,
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    let payload: Vec<u8> = (0..=255_u8).cycle().take(1024).collect();
    supervisor.write(&payload);

    assert!(
        wait_for(|| device.lock().unwrap().written.len() == payload.len()),
        "every byte arrives, eight at a time"
    );
    assert_eq!(
        device.lock().unwrap().written,
        payload,
        "and in the order they were written"
    );
}

#[test]
fn writes_reach_the_device_and_events_are_reported() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));
    supervisor.write(b"at\r");
    assert!(wait_for(|| device.lock().unwrap().written == b"at\r"));

    supervisor
        .set_rts(LineHold::Down)
        .expect("command accepted");
    assert!(wait_for(|| !device.lock().unwrap().rts));
    assert!(wait_for(|| supervisor.status().lines.cts));

    let mut opened = false;
    while let Some(event) = supervisor.try_event() {
        if matches!(event, PortEvent::Opened { .. }) {
            opened = true;
        }
    }
    assert!(opened);
}

/// A break asked for is put back on a port the worker opened again, the way a
/// hold on a modem line is: the break was asked of the device and not of the
/// handle that happened to be open.
#[test]
fn a_held_break_is_put_back_on_the_line_after_a_reconnect() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));
    assert!(
        !device.lock().unwrap().held_break,
        "a port opens with the line free"
    );

    supervisor.set_break(true).expect("command accepted");
    assert!(wait_for(|| device.lock().unwrap().held_break));
    assert!(supervisor.status().held_break);

    {
        let mut guard = device.lock().unwrap();
        guard.present = false;
        guard.fail_next_read = true;
        guard.held_break = false;
    }
    assert!(wait_for(
        || supervisor.status().state == PortState::Disconnected
    ));
    device.lock().unwrap().present = true;
    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));

    assert!(
        wait_for(|| device.lock().unwrap().held_break),
        "the break outlives the connection it was asked for"
    );
}

/// The queue of the driver is reported apart from what is waiting for the line.
///
/// The two are not one answer: what waits for the line is everything on this
/// side of it, the buffer of this program and the queue of the driver together,
/// which says whether a transfer is over; the queue itself says where the bytes
/// are standing.
#[test]
fn the_status_carries_the_queue_of_the_driver_on_its_own() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        pending_write: 7,
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));

    assert!(wait_for(|| supervisor.status().output_queue == 7));
    assert_eq!(
        supervisor.status().input_queue,
        0,
        "nothing was said by the device"
    );

    // A port that is gone carries no queue: the numbers are of an open handle.
    {
        let mut guard = device.lock().unwrap();
        guard.present = false;
        guard.fail_next_read = true;
    }
    assert!(wait_for(
        || supervisor.status().state == PortState::Disconnected
    ));
    assert_eq!(supervisor.status().output_queue, 0);
}

/// One poll of the lines is one sample, whether the lines moved or not.
///
/// The picture drawn from the history is a track over time, and a track
/// carrying a sample only where something changed has no time on its axis at
/// all: a line that stood still for a minute would take the same width as one
/// that stood still for a second.
#[test]
fn a_poll_that_changed_nothing_still_leaves_a_sample() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));

    // The fake device raises CTS once, when it is opened, and never moves a
    // line again — so it emits one `PortEvent::Lines` and no more. Several
    // samples carrying that level is the whole claim: they were taken by the
    // clock and not by a change.
    let mut standing = 0;
    let mut samples = Vec::new();
    assert!(wait_for(|| {
        supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
        standing = samples
            .iter()
            .filter(|sample| sample.has(Signal::Cts))
            .count();
        standing >= 3
    }));

    assert!(
        standing >= 3,
        "a poll that moved nothing still left a sample"
    );
}

/// A sample says that a byte crossed since the poll before it, and says it
/// once.
///
/// The flag is cleared when the sample is taken, so one byte marks the sample
/// it crossed in and leaves every later one alone — otherwise a line that said
/// one word would read as a line that never stopped talking.
#[test]
fn a_byte_that_crossed_marks_the_sample_it_crossed_in_and_no_later_one() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));
    supervisor.write(b"a word");

    let mut samples = Vec::new();
    assert!(
        wait_for(|| {
            supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
            samples.iter().any(|sample| sample.has(Signal::Sent))
        }),
        "the byte that went out marks a sample"
    );

    let marked = samples
        .iter()
        .filter(|sample| sample.has(Signal::Sent))
        .count();
    assert_eq!(marked, 1, "and only the one it crossed in");

    assert!(wait_for(|| {
        supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
        matches!(samples.last(), Some(last) if !last.has(Signal::Sent))
    }));
}

/// A device that went away is samples of nothing and not a gap in the track.
///
/// The history outlives the connection on purpose: the stretch just before a
/// device stopped answering is the one somebody opens this history to look at,
/// and a history cleared on the next open would wipe it. The time the device
/// was away has to be visible as itself, so the samples of it are taken too and
/// every signal in them is down.
#[test]
fn a_device_that_went_away_is_sampled_as_nothing() {
    let device = Arc::new(Mutex::new(Device {
        present: true,
        path: "/dev/ttyUSB0".to_string(),
        ..Device::default()
    }));
    let supervisor = PortSupervisor::spawn(
        config(),
        Box::new(FakeBackend {
            device: device.clone(),
        }),
        None,
    )
    .expect("worker starts");

    assert!(wait_for(
        || supervisor.status().state == PortState::Connected
    ));
    let mut samples = Vec::new();
    assert!(wait_for(|| {
        supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
        samples.iter().any(|sample| sample.has(Signal::Cts))
    }));
    let before = samples.len();

    {
        let mut guard = device.lock().unwrap();
        guard.present = false;
        guard.fail_next_read = true;
    }
    assert!(wait_for(
        || supervisor.status().state == PortState::Disconnected
    ));

    assert!(
        wait_for(|| {
            supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
            matches!(samples.last(), Some(last) if !last.has(Signal::Cts))
        }),
        "a port that is gone is sampled as every signal down"
    );

    supervisor.history(zyt_serial::LINE_HISTORY_SAMPLES, &mut samples);
    assert!(
        samples.len() > before && samples.iter().any(|sample| sample.has(Signal::Cts)),
        "and what the lines did before it went is still there"
    );
}
