//! Line parameters of a serial port. Own types, serde friendly; what a driver
//! makes of them is the business of the platform module that talks to it.

use serde::{Deserialize, Serialize};

/// Number of data bits per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataBits {
    /// Five data bits.
    Five,
    /// Six data bits.
    Six,
    /// Seven data bits.
    Seven,
    /// Eight data bits.
    Eight,
}

/// Parity check mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    /// No parity bit.
    None,
    /// Odd parity.
    Odd,
    /// Even parity.
    Even,
}

/// Number of stop bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopBits {
    /// One stop bit.
    One,
    /// Two stop bits.
    Two,
}

/// Flow control mode of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowControl {
    /// No flow control.
    None,
    /// XON/XOFF software flow control.
    Software,
    /// RTS/CTS hardware flow control.
    Hardware,
    /// RTS/CTS and XON/XOFF at once.
    ///
    /// The two live in different flag words of a line and neither switches the
    /// other on, so a device that answers one of them and a device that answers
    /// the other are both held back by a line carrying both.
    Both,
}

/// Full set of line parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineParams {
    /// Baud rate in bits per second.
    pub baud_rate: u32,
    /// Data bits per character.
    pub data_bits: DataBits,
    /// Parity mode.
    pub parity: Parity,
    /// Stop bits.
    pub stop_bits: StopBits,
    /// Flow control mode.
    pub flow_control: FlowControl,
    /// Whether the driver buffers are emptied when the port opens.
    ///
    /// A device that talked while nothing had it open left its words in the
    /// driver, and a session that starts by reading them starts in the middle
    /// of a sentence nobody asked for.
    #[serde(default = "flush_on_open")]
    pub flush_on_open: bool,
    /// Whether the driver drops the modem lines when the port closes (`HUPCL`).
    ///
    /// Dropping them is what tells the device at the far end that the session
    /// is over, which is what most of them are waiting for.
    #[serde(default = "hupcl")]
    pub hupcl: bool,
}

/// Whether the driver buffers are emptied on open, before anything says
/// otherwise.
fn flush_on_open() -> bool {
    true
}

/// Whether the modem lines are dropped on close, before anything says
/// otherwise.
fn hupcl() -> bool {
    true
}

/// Whether this platform has the hang up flag at all.
///
/// `HUPCL` is a `termios` flag and Windows has no `termios`: what its driver
/// does with the lines when a handle closes is the driver's business, and no
/// setting of this crate reaches it. A window hides the control where this is
/// false rather than offering one that decides nothing.
pub const HUPCL_SUPPORTED: bool = cfg!(target_os = "linux");

impl Default for LineParams {
    fn default() -> Self {
        Self {
            baud_rate: 115_200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            flush_on_open: flush_on_open(),
            hupcl: hupcl(),
        }
    }
}

/// Baud rates offered by the user interface.
pub const COMMON_BAUD_RATES: [u32; 14] = [
    300, 1200, 2400, 4800, 9600, 19_200, 38_400, 57_600, 115_200, 230_400, 460_800, 921_600,
    1_000_000, 2_000_000,
];

impl LineParams {
    /// Short textual form used by the status bar, e.g. `115200 8N1`.
    pub fn summary(&self) -> String {
        let data = match self.data_bits {
            DataBits::Five => '5',
            DataBits::Six => '6',
            DataBits::Seven => '7',
            DataBits::Eight => '8',
        };
        let parity = match self.parity {
            Parity::None => 'N',
            Parity::Odd => 'O',
            Parity::Even => 'E',
        };
        let stop = match self.stop_bits {
            StopBits::One => '1',
            StopBits::Two => '2',
        };
        format!("{} {}{}{}", self.baud_rate, data, parity, stop)
    }
}
