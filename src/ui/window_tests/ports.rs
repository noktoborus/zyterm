//! The menu of the sources with two ports of one USB device.

use egui_kittest::kittest::Queryable;
use zyt_serial::{PortInfo, PortKind, UsbInfo};

use super::window;

/// A port of the board of two CDC interfaces: one device, one serial number.
fn lilygo(path: &str, interface: &str) -> PortInfo {
    PortInfo {
        path: path.to_string(),
        kind: PortKind::Usb,
        usb: Some(UsbInfo {
            vid: 0x1209,
            pid: 0x0001,
            serial_number: Some("SN1".to_string()),
            manufacturer: Some("LILYGO".to_string()),
            product: Some("t-display-s3-uart".to_string()),
            interface: Some(interface.to_string()),
        }),
        accessible: true,
    }
}

/// Two ports of one device are two entries, and the second one opens the
/// second port: a port is its path (`RULE port.identity`).
#[test]
fn the_second_port_of_one_device_is_the_one_opened() {
    let mut window = window();
    {
        let app = window.harness.state_mut();
        app.ports = vec![
            lilygo("/dev/ttyACM0", "LILYGO Log/Settings"),
            lilygo("/dev/ttyACM1", "LILYGO UART"),
        ];
        let items = crate::ui::connect::items(app);
        app.menu.open(items).expect("the menu opens");
    }
    window.harness.run_steps(2);

    assert!(window.shows("/dev/ttyACM0") && window.shows("/dev/ttyACM1"));
    assert!(
        window
            .harness
            .query_by_label_contains("LILYGO t-display-s3-uart")
            .is_some(),
        "the plate names the model"
    );
    assert!(
        window.harness.query_by_label_contains("SN1").is_some(),
        "the plate carries the serial number"
    );

    window.harness.get_by_label("/dev/ttyACM1").click();
    window.harness.run_steps(4);
    assert_eq!(
        window.harness.state().session.port_path(),
        Some("/dev/ttyACM1")
    );
}

/// A speed the driver rounds: the status bar shows the line the driver has,
/// and the menu of the line says it.
#[test]
fn the_status_bar_shows_the_line_the_driver_has() {
    let mut window = window();
    window.device.lock().unwrap().only_speed = Some(230_400);
    window
        .harness
        .state_mut()
        .set_line_params(zyt_serial::LineParams {
            baud_rate: 250_000,
            ..zyt_serial::LineParams::default()
        });
    window.step_until("the rounded speed", |window| window.shows("230400 8N1"));

    window.harness.get_by_label("230400 8N1").click();
    window.harness.run_steps(2);
    assert!(
        window
            .harness
            .query_by_label_contains("230400 8N1 now")
            .is_some(),
        "the menu of the line says what the port has"
    );
}
