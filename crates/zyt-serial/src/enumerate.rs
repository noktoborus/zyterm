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

/// USB descriptor fields of a port. They name the port and identify nothing:
/// two ports of one device carry the same vendor, product and serial number.
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
    /// String of the interface the port is on (iInterface), when the device
    /// reports one: what tells apart two ports of one device.
    pub interface: Option<String>,
}

/// Which port a caller means: the path the kernel gave it.
///
/// A device that comes back is given its old number when nothing holds its
/// node, so the path is what a port is opened at again. It is an enum of
/// kinds, so a port that is not a device node is one more kind.
///
/// Its text form is what a configuration file carries — `path:/dev/…` — so
/// that is what it is written as and read back from, rather than a map of its
/// fields: a key is read by whoever opens the file, and a line of text is a
/// key nobody has to decode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum PortId {
    /// Operating system path, e.g. `/dev/ttyACM1` or `COM3`.
    Path(String),
}

/// The word in front of the text form of a path.
const PATH_KIND: &str = "path";

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
            Self::Path(path) => write!(formatter, "{PATH_KIND}:{path}"),
        }
    }
}

impl std::str::FromStr for PortId {
    type Err = PortError;

    /// Reads back what [`std::fmt::Display`] wrote.
    ///
    /// A path is the rest of the text, so a path with a colon in it comes back
    /// as it went in. A kind this version does not know is
    /// [`PortError::Kind`], apart from a text that is no key at all.
    fn from_str(text: &str) -> Result<Self> {
        let key = || PortError::Key {
            text: text.to_string(),
        };
        let (kind, rest) = text.split_once(':').ok_or_else(key)?;
        if kind.is_empty() || rest.is_empty() {
            return Err(key());
        }
        match kind {
            PATH_KIND => Ok(Self::Path(rest.to_string())),
            _ => Err(PortError::Kind {
                text: text.to_string(),
            }),
        }
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
    /// Which port this is.
    pub fn id(&self) -> PortId {
        PortId::Path(self.path.clone())
    }

    /// Human readable label: path plus product string when available.
    pub fn label(&self) -> String {
        match self.usb.as_ref().and_then(|usb| usb.product.as_ref()) {
            Some(product) => format!("{} ({product})", self.path),
            None => self.path.clone(),
        }
    }

    /// Name the device resolved to: the string of its interface when the
    /// device reports one, else the manufacturer and the product, else the
    /// USB identifiers, and the transport for everything else.
    ///
    /// The string of the interface is not joined to the other two: it often
    /// carries the manufacturer already.
    pub fn resolved_name(&self) -> String {
        let Some(usb) = &self.usb else {
            return match self.kind {
                PortKind::Builtin => "UART".to_string(),
                PortKind::Bluetooth => "Bluetooth".to_string(),
                PortKind::Usb => "USB".to_string(),
                PortKind::Unknown => "TTY".to_string(),
            };
        };
        if let Some(interface) = usb.interface.as_deref().map(str::trim)
            && !interface.is_empty()
        {
            return interface.to_string();
        }
        match self.model() {
            Some(model) => model,
            None => format!("USB {:04x}:{:04x}", usb.vid, usb.pid),
        }
    }

    /// The manufacturer and the product of a USB device, when it reports
    /// either.
    pub fn model(&self) -> Option<String> {
        let usb = self.usb.as_ref()?;
        let parts: Vec<&str> = [&usb.manufacturer, &usb.product]
            .into_iter()
            .flatten()
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.join(" "))
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

/// Where the nodes of the ttys are.
#[cfg(target_os = "linux")]
const DEV: &str = "/dev";

/// Lists the ports of the system.
#[cfg(target_os = "linux")]
fn scan() -> Result<Vec<PortInfo>> {
    scan_in(std::path::Path::new(TTY_CLASS), std::path::Path::new(DEV))
}

/// Lists the ports by walking the class of the ttys, `/sys/class/tty`.
///
/// A tty backed by hardware carries a `device` entry and a virtual console does
/// not, which is the whole filter: it catches `ttyUSB*`, `ttyACM*`, `ttyS*` and
/// every driver that names its ports otherwise, and it needs no udev database
/// — there is none inside a container.
#[cfg(target_os = "linux")]
fn scan_in(tty_class: &std::path::Path, dev: &std::path::Path) -> Result<Vec<PortInfo>> {
    let entries = std::fs::read_dir(tty_class).map_err(|source| PortError::Enumerate { source })?;

    let mut ports = Vec::new();
    for entry in entries.flatten() {
        let class = entry.path();
        if !class.join("device").exists() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let node = dev.join(&name);
        if !node.exists() {
            continue;
        }
        let path = node.to_string_lossy().to_string();

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
///
/// The string of the interface is taken from the first node on the way that
/// has one: for `ttyACM*` it is the node `device` itself, for `ttyUSB*` the one
/// above it.
#[cfg(target_os = "linux")]
fn read_usb_info(class_path: &std::path::Path) -> Option<UsbInfo> {
    let mut device = std::fs::canonicalize(class_path.join("device")).ok()?;
    let mut interface = None;
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
                interface,
            });
        }
        interface = interface.or_else(|| read_text(&device.join("interface")));
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
                interface: None,
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::path::Path;

    /// A tree that looks like sysfs and `/dev` to the walk.
    struct Tree {
        root: tempfile::TempDir,
    }

    impl Tree {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("a scratch directory");
            for dir in ["class", "dev", "devices"] {
                std::fs::create_dir(root.path().join(dir)).expect("a directory");
            }
            Self { root }
        }

        fn path(&self, path: &str) -> std::path::PathBuf {
            self.root.path().join(path)
        }

        fn file(&self, path: &str, text: &str) {
            let path = self.path(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(path, format!("{text}\n")).expect("a file");
        }

        /// A tty whose `device` is the node at `device`, below `devices`.
        fn tty(&self, name: &str, device: &str) {
            let node = self.path(&format!("devices/{device}"));
            std::fs::create_dir_all(&node).expect("a node");
            let class = self.path(&format!("class/{name}"));
            std::fs::create_dir(&class).expect("a class entry");
            std::os::unix::fs::symlink(&node, class.join("device")).expect("a link");
            self.file(&format!("dev/{name}"), "");
        }

        fn usb_device(&self, at: &str, vid: &str, pid: &str) {
            self.file(&format!("devices/{at}/idVendor"), vid);
            self.file(&format!("devices/{at}/idProduct"), pid);
        }

        fn scan(&self) -> Vec<PortInfo> {
            let mut ports =
                scan_in(&self.path("class"), &self.path("dev")).expect("the walk works");
            ports.sort_by(|a, b| a.path.cmp(&b.path));
            ports
        }
    }

    fn named<'a>(ports: &'a [PortInfo], name: &str) -> &'a PortInfo {
        ports
            .iter()
            .find(|port| Path::new(&port.path).ends_with(name))
            .unwrap_or_else(|| panic!("{name} is listed"))
    }

    /// Two ports of one device are two ports, each named by its interface.
    #[test]
    fn two_ports_of_one_device_are_told_apart_by_path_and_interface() {
        let tree = Tree::new();
        tree.usb_device("usb5/5-1", "1209", "0001");
        tree.file("devices/usb5/5-1/manufacturer", "LILYGO");
        tree.file("devices/usb5/5-1/product", "t-display-s3-uart");
        tree.tty("ttyACM0", "usb5/5-1/5-1:1.0");
        tree.file("devices/usb5/5-1/5-1:1.0/interface", "LILYGO Log/Settings");
        tree.tty("ttyACM1", "usb5/5-1/5-1:1.2");
        tree.file("devices/usb5/5-1/5-1:1.2/interface", "LILYGO UART");

        let ports = tree.scan();
        let (log, uart) = (named(&ports, "ttyACM0"), named(&ports, "ttyACM1"));
        assert_ne!(log.id(), uart.id());
        assert_eq!(log.resolved_name(), "LILYGO Log/Settings");
        assert_eq!(uart.resolved_name(), "LILYGO UART");
        assert_eq!(uart.model().as_deref(), Some("LILYGO t-display-s3-uart"));
        assert_eq!(uart.kind, PortKind::Usb);
    }

    /// A `ttyUSB` port hangs one level below its interface.
    #[test]
    fn the_interface_of_a_usb_serial_port_is_one_level_up() {
        let tree = Tree::new();
        tree.usb_device("usb3/3-1", "067b", "2303");
        tree.file("devices/usb3/3-1/product", "USB-Serial Controller D");
        tree.file("devices/usb3/3-1/3-1:1.0/interface", "Serial Port");
        tree.tty("ttyUSB0", "usb3/3-1/3-1:1.0/ttyUSB0");

        let ports = tree.scan();
        let port = named(&ports, "ttyUSB0");
        assert_eq!(
            port.usb.as_ref().and_then(|usb| usb.interface.as_deref()),
            Some("Serial Port")
        );
        assert_eq!(port.resolved_name(), "Serial Port");
    }

    /// No strings at all: the USB identifiers; no USB at all: the transport.
    #[test]
    fn a_port_with_no_strings_is_named_by_what_is_left() {
        let tree = Tree::new();
        tree.usb_device("usb1/1-2", "1a86", "7523");
        tree.tty("ttyUSB1", "usb1/1-2/1-2:1.0/ttyUSB1");
        tree.tty("ttyS0", "pnp0/00:00");

        let ports = tree.scan();
        assert_eq!(named(&ports, "ttyUSB1").resolved_name(), "USB 1a86:7523");
        assert_eq!(named(&ports, "ttyUSB1").model(), None);
        let uart = named(&ports, "ttyS0");
        assert!(uart.usb.is_none());
        assert_eq!(uart.kind, PortKind::Builtin);
        assert_eq!(uart.resolved_name(), "UART");
    }

    /// A tty with no `device` is a virtual console; one with no node is gone.
    #[test]
    fn a_virtual_console_and_a_tty_with_no_node_are_not_ports() {
        let tree = Tree::new();
        std::fs::create_dir(tree.path("class/tty1")).expect("a class entry");
        tree.tty("ttyS1", "platform/serial8250");
        std::fs::remove_file(tree.path("dev/ttyS1")).expect("the node goes");

        assert!(tree.scan().is_empty());
    }
}
