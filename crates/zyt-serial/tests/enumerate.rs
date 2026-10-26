//! Enumeration on the running system.

use zyt_serial::{PortError, PortId, accessible_ports, available_ports};

#[test]
fn enumeration_does_not_fail() {
    let ports = available_ports().expect("enumeration works");
    for port in &ports {
        assert!(!port.path.is_empty());
        assert!(!port.resolved_name().is_empty());
        assert_eq!(port.id(), PortId::Path(port.path.clone()));
    }
    let accessible = accessible_ports().expect("filtered enumeration works");
    assert!(accessible.len() <= ports.len());
}

/// The text form is what a configuration file carries, so what was written is
/// what comes back — a path with a colon in it included, because it is the
/// rest of the text and not one field among several.
#[test]
fn a_port_comes_back_from_its_text() {
    for path in ["/dev/ttyACM1", "COM3", "/dev/serial:odd"] {
        let id = PortId::Path(path.to_string());
        let text = id.to_string();
        assert_eq!(text, format!("path:{path}"));
        assert_eq!(text.parse::<PortId>().expect("the text reads"), id);
    }
}

/// A text that names no port is an error and never a device nobody has.
#[test]
fn a_text_that_names_no_port_is_refused() {
    for text in ["", "path:", ":/dev/ttyS0", "/dev/ttyS0"] {
        assert!(
            matches!(text.parse::<PortId>(), Err(PortError::Key { .. })),
            "{text}"
        );
    }
}

/// A kind this version does not know is told apart from a broken text, so a
/// file of a later version is passed over and not taken for a broken one.
#[test]
fn a_kind_this_version_does_not_know_is_its_own_error() {
    for text in ["usb:0403:6001:A1", "telnet:host:2217"] {
        assert!(
            matches!(text.parse::<PortId>(), Err(PortError::Kind { .. })),
            "{text}"
        );
    }
}
