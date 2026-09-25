//! Port discovery and stable device identity.

use crate::error::{PortError, Result};
use serde::{Deserialize, Serialize};

/// Transport a port is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    /// USB serial adapter or USB CDC device.
    Usb,
    /// PCI or on board UART.
    Builtin,
    /// Bluetooth serial profile.
    Bluetooth,
    /// Kind could not be determined.
    Unknown,
}

/// USB descriptor fields of a port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbInfo {
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Device serial number, when the device reports one.
    pub serial_number: Option<String>,
    /// Manufacturer string.
    pub manufacturer: Option<String>,
    /// Product string.
    pub product: Option<String>,
}

/// Identity that survives replugging. USB devices are identified by their
/// descriptors, everything else by the operating system path.
///
/// Its text form is what a configuration file carries — `usb:vid:pid:serial` or
/// `path:/dev/…` — so that is what it is written as and read back from, rather
/// than a map of its fields: a key is read by whoever opens the file, and two
/// lines of text are a key nobody has to decode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum PortId {
    /// USB device identity.
    Usb {
        /// USB vendor id.
        vid: u16,
        /// USB product id.
        pid: u16,
        /// Device serial number, when the device reports one.
        serial_number: Option<String>,
    },
    /// Operating system path, used when no stable identity exists.
    Path(String),
}

/// One port found in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortInfo {
    /// Operating system path, e.g. `/dev/ttyUSB0` or `COM3`.
    pub path: String,
    /// Transport of the port.
    pub kind: PortKind,
    /// USB descriptors when the port is a USB device.
    pub usb: Option<UsbInfo>,
    /// The current process may open the port for reading and writing.
    pub accessible: bool,
}

impl std::fmt::Display for PortId {
    /// The text form of the identity, which is what a configuration file
    /// carries and what [`PortId::from_str`] reads.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb {
                vid,
                pid,
                serial_number,
            } => match serial_number {
                Some(serial) => write!(formatter, "usb:{vid:04x}:{pid:04x}:{serial}"),
                None => write!(formatter, "usb:{vid:04x}:{pid:04x}"),
            },
            Self::Path(path) => write!(formatter, "path:{path}"),
        }
    }
}

impl std::str::FromStr for PortId {
    type Err = PortError;

    /// Reads back what [`std::fmt::Display`] wrote.
    ///
    /// A serial number is the rest of the text and not one field among several,
    /// so a device whose serial number carries a colon comes back as it went
    /// in. A path is the rest of the text for the same reason.
    fn from_str(text: &str) -> Result<Self> {
        let key = || PortError::Key {
            text: text.to_string(),
        };

        if let Some(path) = text.strip_prefix("path:") {
            if path.is_empty() {
                return Err(key());
            }
            return Ok(Self::Path(path.to_string()));
        }

        let usb = text.strip_prefix("usb:").ok_or_else(key)?;
        let mut parts = usb.splitn(3, ':');
        let vid = parts.next().ok_or_else(key)?;
        let pid = parts.next().ok_or_else(key)?;
        Ok(Self::Usb {
            vid: u16::from_str_radix(vid, 16).map_err(|_| key())?,
            pid: u16::from_str_radix(pid, 16).map_err(|_| key())?,
            serial_number: parts.next().map(str::to_string),
        })
    }
}

impl From<PortId> for String {
    fn from(id: PortId) -> Self {
        id.to_string()
    }
}

impl TryFrom<String> for PortId {
    type Error = PortError;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

impl PortInfo {
    /// Identity of this port that survives replugging.
    pub fn id(&self) -> PortId {
        match &self.usb {
            Some(usb) => PortId::Usb {
                vid: usb.vid,
                pid: usb.pid,
                serial_number: usb.serial_number.clone(),
            },
            None => PortId::Path(self.path.clone()),
        }
    }

    /// True when this port matches the given identity.
    pub fn matches(&self, id: &PortId) -> bool {
        match id {
            PortId::Path(path) => &self.path == path,
            PortId::Usb { .. } => &self.id() == id,
        }
    }

    /// Human readable label: path plus product string when available.
    pub fn label(&self) -> String {
        match self.usb.as_ref().and_then(|usb| usb.product.as_ref()) {
            Some(product) => format!("{} ({product})", self.path),
            None => self.path.clone(),
        }
    }

    /// Name the device resolved to: product and manufacturer when the device
    /// reports them, the USB identifiers otherwise, and the transport for
    /// everything else.
    pub fn resolved_name(&self) -> String {
        let Some(usb) = &self.usb else {
            return match self.kind {
                PortKind::Builtin => "UART".to_string(),
                PortKind::Bluetooth => "Bluetooth".to_string(),
                PortKind::Usb => "USB".to_string(),
                PortKind::Unknown => "TTY".to_string(),
            };
        };

        let mut parts = Vec::new();
        if let Some(manufacturer) = &usb.manufacturer {
            parts.push(manufacturer.trim().to_string());
        }
        if let Some(product) = &usb.product {
            parts.push(product.trim().to_string());
        }
        parts.retain(|part| !part.is_empty());
        if parts.is_empty() {
            return format!("USB {:04x}:{:04x}", usb.vid, usb.pid);
        }
        parts.join(" ")
    }
}

/// Lists all serial ports of the system.
///
/// What answers this is the platform: a scan of `/sys/class/tty` on Linux, the
/// driver crate on Windows.
pub fn available_ports() -> Result<Vec<PortInfo>> {
    let mut ports = scan()?;
    for port in &mut ports {
        port.accessible = is_accessible(&port.path);
    }
    ports.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ports)
}

/// Ports the current process is allowed to open.
pub fn accessible_ports() -> Result<Vec<PortInfo>> {
    Ok(available_ports()?
        .into_iter()
        .filter(|port| port.accessible)
        .collect())
}

#[cfg(unix)]
fn is_accessible(path: &str) -> bool {
    let Ok(text) = std::ffi::CString::new(path) else {
        return false;
    };
    // Safety: the name is a NUL terminated path held for the whole call.
    unsafe { libc::access(text.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn is_accessible(_path: &str) -> bool {
    true
}

/// Where the kernel lists every tty it has, real hardware and virtual console
/// alike.
#[cfg(target_os = "linux")]
const TTY_CLASS: &str = "/sys/class/tty";

/// Lists the ports by walking `/sys/class/tty`.
///
/// A tty backed by hardware carries a `device` entry and a virtual console does
/// not, which is the whole filter: it catches `ttyUSB*`, `ttyACM*`, `ttyS*` and
/// every driver that names its ports otherwise, and it needs no udev database
/// — there is none inside a container.
#[cfg(target_os = "linux")]
fn scan() -> Result<Vec<PortInfo>> {
    let entries = std::fs::read_dir(TTY_CLASS).map_err(|source| PortError::Enumerate { source })?;

    let mut ports = Vec::new();
    for entry in entries.flatten() {
        let class = entry.path();
        if !class.join("device").exists() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let path = format!("/dev/{name}");
        if !std::path::Path::new(&path).exists() {
            continue;
        }

        let usb = read_usb_info(&class);
        ports.push(PortInfo {
            kind: kind_of(&name, usb.is_some()),
            path,
            usb,
            accessible: false,
        });
    }
    Ok(ports)
}

/// What a port is attached to, as far as its name and its descriptors say.
#[cfg(target_os = "linux")]
fn kind_of(name: &str, usb: bool) -> PortKind {
    if usb || name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
        PortKind::Usb
    } else if name.starts_with("rfcomm") {
        PortKind::Bluetooth
    } else {
        PortKind::Builtin
    }
}

/// Walks up the device tree of a tty until a node carries USB descriptors.
#[cfg(target_os = "linux")]
fn read_usb_info(class_path: &std::path::Path) -> Option<UsbInfo> {
    let mut device = std::fs::canonicalize(class_path.join("device")).ok()?;
    for _ in 0..6 {
        let vid = read_hex(&device.join("idVendor"));
        let pid = read_hex(&device.join("idProduct"));
        if let (Some(vid), Some(pid)) = (vid, pid) {
            return Some(UsbInfo {
                vid,
                pid,
                serial_number: read_text(&device.join("serial")),
                manufacturer: read_text(&device.join("manufacturer")),
                product: read_text(&device.join("product")),
            });
        }
        device = device.parent()?.to_path_buf();
    }
    None
}

#[cfg(target_os = "linux")]
fn read_text(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(target_os = "linux")]
fn read_hex(path: &std::path::Path) -> Option<u16> {
    u16::from_str_radix(read_text(path)?.as_str(), 16).ok()
}

/// Lists the ports the driver crate reports.
#[cfg(windows)]
fn scan() -> Result<Vec<PortInfo>> {
    let found = serialport::available_ports().map_err(|source| PortError::Enumerate {
        source: source.into(),
    })?;
    Ok(found.into_iter().map(convert).collect())
}

#[cfg(windows)]
fn convert(info: serialport::SerialPortInfo) -> PortInfo {
    match info.port_type {
        serialport::SerialPortType::UsbPort(usb) => PortInfo {
            path: info.port_name,
            kind: PortKind::Usb,
            usb: Some(UsbInfo {
                vid: usb.vid,
                pid: usb.pid,
                serial_number: usb.serial_number,
                manufacturer: usb.manufacturer,
                product: usb.product,
            }),
            accessible: false,
        },
        serialport::SerialPortType::BluetoothPort => PortInfo {
            path: info.port_name,
            kind: PortKind::Bluetooth,
            usb: None,
            accessible: false,
        },
        serialport::SerialPortType::PciPort => PortInfo {
            path: info.port_name,
            kind: PortKind::Builtin,
            usb: None,
            accessible: false,
        },
        serialport::SerialPortType::Unknown => PortInfo {
            path: info.port_name,
            kind: PortKind::Unknown,
            usb: None,
            accessible: false,
        },
    }
}
