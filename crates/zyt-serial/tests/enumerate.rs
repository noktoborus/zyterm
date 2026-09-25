//! Enumeration on the running system.

use zyt_serial::{PortId, accessible_ports, available_ports};

#[test]
fn enumeration_does_not_fail() {
    let ports = available_ports().expect("enumeration works");
    for port in &ports {
        assert!(!port.path.is_empty());
        assert!(!port.resolved_name().is_empty());
        let key = port.id().to_string();
        assert!(key.starts_with("usb:") || key.starts_with("path:"));
    }
    let accessible = accessible_ports().expect("filtered enumeration works");
    assert!(accessible.len() <= ports.len());
}

#[test]
fn port_key_is_stable_and_distinct() {
    let usb = PortId::Usb {
        vid: 0x0403,
        pid: 0x6001,
        serial_number: Some("A1".to_string()),
    };
    assert_eq!(usb.to_string(), "usb:0403:6001:A1");
    assert_eq!(
        PortId::Path("/dev/ttyS0".to_string()).to_string(),
        "path:/dev/ttyS0"
    );
    assert_ne!(
        usb.to_string(),
        PortId::Path("/dev/ttyUSB0".to_string()).to_string()
    );
}

/// The text form is what a configuration file carries, so what was written is
/// what comes back — a serial number with a colon in it included, because it is
/// the rest of the text and not one field among several.
#[test]
fn an_identity_comes_back_from_its_text() {
    for id in [
        PortId::Usb {
            vid: 0x0403,
            pid: 0x6001,
            serial_number: Some("A1".to_string()),
        },
        PortId::Usb {
            vid: 0x1a86,
            pid: 0x7523,
            serial_number: None,
        },
        PortId::Usb {
            vid: 0x0403,
            pid: 0x6001,
            serial_number: Some("A1:B2".to_string()),
        },
        PortId::Path("/dev/ttyS0".to_string()),
        PortId::Path("COM3".to_string()),
    ] {
        let text = id.to_string();
        assert_eq!(
            text.parse::<PortId>().expect("the text reads"),
            id,
            "{text}"
        );
    }
}

/// A text that names no identity is an error and never a device nobody has.
#[test]
fn a_text_that_names_no_identity_is_refused() {
    for text in [
        "",
        "usb:",
        "path:",
        "/dev/ttyS0",
        "usb:zzzz:6001",
        "usb:0403",
    ] {
        assert!(text.parse::<PortId>().is_err(), "{text}");
    }
}
