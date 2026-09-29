//! Serial port access for terminal applications.
//!
//! The crate does three things and nothing else:
//!
//! * enumerate the serial ports of the system with a stable device identity,
//! * open one port in a worker thread and hand the bytes to another thread
//!   through a swapped buffer instead of a queue,
//! * survive unplugging: the handle is dropped as soon as the device is gone,
//!   so a returning device can reuse the same device node, and the worker
//!   reopens it by identity.
//!
//! The driver sits behind one module per platform: `tty` talks to the Linux
//! descriptor directly, `comm` drives the `serialport` crate on Windows. Those
//! two modules are the only place a driver is named.
//!
//! The crate has no dependency on any user interface.

#![deny(missing_docs)]

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("zyt-serial is built for linux and windows");

mod backend;
#[cfg(windows)]
mod comm;
mod enumerate;
mod error;
mod history;
mod lines;
mod params;
mod rxbuf;
mod supervisor;
#[cfg(target_os = "linux")]
mod tty;

pub use backend::{PortBackend, PortHandle, SystemBackend};
pub use enumerate::{PortId, PortInfo, PortKind, UsbInfo, accessible_ports, available_ports};
pub use error::{PortError, Result};
pub use history::{LINE_HISTORY_SAMPLES, LineHistory, LineSample, LineScale, Signal};
pub use lines::{ControlLines, LineEdges, LineHold, LineHolds};
pub use params::{
    COMMON_BAUD_RATES, DataBits, FlowControl, HUPCL_SUPPORTED, LineParams, Parity, StopBits,
};
pub use rxbuf::ByteSwap;
pub use supervisor::{
    DEFAULT_LINES_INTERVAL, LINES_INTERVAL_RANGE, Notify, PortEvent, PortState, PortStatus,
    PortSupervisor, SupervisorConfig,
};
