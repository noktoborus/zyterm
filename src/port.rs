//! The serial port of the session as a run drives it (`zyt.port`): from the
//! thread of the script, with no frame of the window on the way. What the run
//! changed is kept for the window, which shows it at its next frame and writes
//! the parameters down when the run ends.

use std::sync::{Arc, Mutex};
use zyt_script::{PortLines, PortParams, PortSwitch};
use zyt_serial::{DataBits, FlowControl, LineHold, LineParams, Parity, PortSupervisor, StopBits};

/// The names a script gives the values of the line.
const DATA_BITS: [(u8, DataBits); 4] = [
    (5, DataBits::Five),
    (6, DataBits::Six),
    (7, DataBits::Seven),
    (8, DataBits::Eight),
];
const PARITY: [(&str, Parity); 3] = [
    ("none", Parity::None),
    ("odd", Parity::Odd),
    ("even", Parity::Even),
];
const STOP_BITS: [(u8, StopBits); 2] = [(1, StopBits::One), (2, StopBits::Two)];
const FLOW: [(&str, FlowControl); 4] = [
    ("none", FlowControl::None),
    ("software", FlowControl::Software),
    ("hardware", FlowControl::Hardware),
    ("both", FlowControl::Both),
];

/// The name of a value; every value of the line has one.
fn key<K: Copy, V: PartialEq>(table: &[(K, V)], value: V) -> K {
    table
        .iter()
        .find(|(_, known)| *known == value)
        .map(|(name, _)| *name)
        .expect("every value of the line is named")
}

/// The value of a name, when it names one.
fn value<K: PartialEq, V: Copy>(table: &[(K, V)], key: K) -> Option<V> {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
}

/// A line in the names a script gives its values.
fn named(params: &LineParams) -> PortParams {
    PortParams {
        baud: params.baud_rate,
        data_bits: key(&DATA_BITS, params.data_bits),
        parity: key(&PARITY, params.parity).to_string(),
        stop_bits: key(&STOP_BITS, params.stop_bits),
        flow: key(&FLOW, params.flow_control).to_string(),
    }
}

/// What a run changed on the port, for the window to take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changed {
    /// The parameters of the line.
    pub params: Option<LineParams>,
    /// The hold of Request To Send.
    pub rts: Option<LineHold>,
    /// The hold of Data Terminal Ready.
    pub dtr: Option<LineHold>,
    /// The break condition.
    pub held_break: Option<bool>,
    /// The hold of the reading.
    pub read_hold: Option<bool>,
}

/// The port of a run.
pub struct ScriptPort {
    supervisor: Arc<PortSupervisor>,
    changed: Arc<Mutex<Changed>>,
}

impl ScriptPort {
    /// The port of `supervisor`; what the run changes goes into `changed`.
    pub fn new(supervisor: Arc<PortSupervisor>, changed: Arc<Mutex<Changed>>) -> Self {
        Self {
            supervisor,
            changed,
        }
    }

    fn change(&self, change: impl FnOnce(&mut Changed)) {
        change(
            &mut self
                .changed
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
    }

    /// The parameters as the driver has them.
    pub fn params(&self) -> PortParams {
        named(&self.supervisor.status().params)
    }

    /// Sets the parameters and waits for the line the driver has after them.
    ///
    /// A value the line has no name for, a speed of nought, a port that is not
    /// open and a line the driver refuses are each an error, in words for the
    /// script.
    pub fn set_params(&self, asked: &PortParams) -> Result<PortParams, String> {
        let mut params = self.supervisor.status().params;
        let (Some(data_bits), Some(parity), Some(stop_bits), Some(flow_control)) = (
            value(&DATA_BITS, asked.data_bits),
            value(&PARITY, asked.parity.as_str()),
            value(&STOP_BITS, asked.stop_bits),
            value(&FLOW, asked.flow.as_str()),
        ) else {
            return Err("a value the line has no name for".to_string());
        };
        if asked.baud == 0 {
            return Err("a speed of nought".to_string());
        }
        params.baud_rate = asked.baud;
        params.data_bits = data_bits;
        params.parity = parity;
        params.stop_bits = stop_bits;
        params.flow_control = flow_control;
        let line = self
            .supervisor
            .apply_params(params)
            .map_err(|error| error.to_string())?;
        self.change(|changed| changed.params = Some(line));
        Ok(named(&line))
    }

    /// Drives the port.
    pub fn switch(&self, switch: PortSwitch) -> bool {
        let hold = |up: Option<bool>| match up {
            Some(true) => LineHold::Up,
            Some(false) => LineHold::Down,
            None => LineHold::Auto,
        };
        let supervisor = &self.supervisor;
        let done = match switch {
            PortSwitch::Rts(up) => supervisor.apply_rts(hold(up)).is_ok(),
            PortSwitch::Dtr(up) => supervisor.apply_dtr(hold(up)).is_ok(),
            PortSwitch::Break(on) => supervisor.set_break(on).is_ok(),
            PortSwitch::Hold(on) => supervisor.set_read_hold(on).is_ok(),
            PortSwitch::Purge => return supervisor.discard_output().is_ok(),
        };
        if done {
            self.change(|changed| match switch {
                PortSwitch::Rts(up) => changed.rts = Some(hold(up)),
                PortSwitch::Dtr(up) => changed.dtr = Some(hold(up)),
                PortSwitch::Break(on) => changed.held_break = Some(on),
                PortSwitch::Hold(on) => changed.read_hold = Some(on),
                PortSwitch::Purge => {}
            });
        }
        done
    }

    /// The lines of the modem as they stand.
    pub fn lines(&self) -> PortLines {
        let lines = self.supervisor.status().lines;
        PortLines {
            rts: lines.rts.unwrap_or(lines.rts_asked),
            dtr: lines.dtr.unwrap_or(lines.dtr_asked),
            cts: lines.cts,
            dsr: lines.dsr,
            cd: lines.cd,
            ri: lines.ri,
        }
    }
}
