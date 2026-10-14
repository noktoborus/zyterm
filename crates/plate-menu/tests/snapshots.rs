//! Pictures of the menu, rendered and compared with the ones in `img/`.
//!
//! Each picture is both a test and an illustration of the README: a change
//! that moves a pixel of the menu fails here, and the README shows what the
//! menu looks like now. `UPDATE_SNAPSHOTS=1 cargo test -p plate-menu --test
//! snapshots` draws them again.

use egui_kittest::{Harness, SnapshotOptions};
use plate_menu::{MenuItem, PlateMenu};

/// Where the pictures are kept: beside the README, which shows them.
const PICTURES: &str = "img";

/// A window wide enough for the plate of a whole beside the menu.
const WIDE: egui::Vec2 = egui::vec2(900.0, 420.0);
/// A window too narrow for it, so the plate stands above the menu.
const NARROW: egui::Vec2 = egui::vec2(480.0, 520.0);

/// A harness drawing `menu` over a window of `size` painted like a panel.
fn harness(menu: PlateMenu, size: egui::Vec2) -> Harness<'static, PlateMenu> {
    Harness::builder()
        .with_size(size)
        .with_theme(egui::Theme::Dark)
        .build_ui_state(
            |ui, menu: &mut PlateMenu| {
                let window = ui.ctx().content_rect();
                ui.ctx()
                    .layer_painter(egui::LayerId::background())
                    .rect_filled(window, 0.0, ui.visuals().panel_fill);
                menu.show(ui.ctx());
            },
            menu,
        )
}

/// Runs the harness until it settles and compares it with `img/<name>.png`.
fn picture(harness: &mut Harness<'_, PlateMenu>, name: &str) {
    harness.run();
    harness.snapshot_options(name, &SnapshotOptions::new().output_path(PICTURES));
}

/// The entries of a terminal: commands with their keys, a level of ports, a
/// disabled entry and more than the menu shows at once.
fn entries() -> Vec<MenuItem> {
    vec![
        MenuItem::new("copy", "Copy").detail("ctrl+shift+c"),
        MenuItem::new("paste", "Paste").detail("ctrl+shift+v"),
        MenuItem::separator(),
        MenuItem::new("port", "Port")
            .detail("ttyUSB0")
            .children(vec![
                MenuItem::new("usb0", "ttyUSB0").detail("115200"),
                MenuItem::new("usb1", "ttyUSB1").detail("9600"),
                MenuItem::new("acm0", "ttyACM0").search("usb modem"),
            ]),
        MenuItem::new("theme", "Theme").children(vec![
            MenuItem::new("dark", "Dark"),
            MenuItem::new("light", "Light"),
        ]),
        MenuItem::new("send", "Send a file").enabled(false),
        MenuItem::new("clear", "Clear the screen"),
        MenuItem::new("search", "Search").detail("ctrl+shift+f"),
        MenuItem::new("quit", "Quit").detail("ctrl+shift+q"),
    ]
}

/// Entries whose labels were cut, each with the whole it was cut from.
fn cut(count: usize) -> Vec<MenuItem> {
    (0..count)
        .map(|index| {
            MenuItem::new(format!("cmd{index}"), format!("make test-{index} && …")).full(format!(
                "make test-{index} && cargo clippy --workspace --all-targets \
                 && cargo test --workspace && cargo audit"
            ))
        })
        .collect()
}

/// Entries whose wholes alternate between one line and many, the first one
/// short.
fn uneven(count: usize) -> Vec<MenuItem> {
    (0..count)
        .map(|index| {
            let whole = match index % 2 {
                0 => format!("make test-{index}"),
                _ => format!("make test-{index} && cargo clippy --workspace --all-targets && ")
                    .repeat(3),
            };
            MenuItem::new(format!("cmd{index}"), format!("make test-{index} …")).full(whole)
        })
        .collect()
}

/// A menu of `items` with nothing else set.
fn opened(items: Vec<MenuItem>) -> PlateMenu {
    let mut menu = PlateMenu::new();
    menu.open(items).expect("an entry can be chosen");
    menu
}

/// The parts of the menu: the notice, the selection, an entry with children,
/// the details, a separator, a disabled entry and the line below the plates.
#[test]
fn the_parts_of_the_menu() {
    let mut menu = opened(entries()).max_plates(7);
    menu.notice("What to do with the session");
    let mut harness = harness(menu, WIDE);
    picture(&mut harness, "parts");
}

/// A level stepped into: the row leading back names the level above.
#[test]
fn a_level_stepped_into() {
    let mut harness = harness(opened(entries()), WIDE);
    harness.run();
    for _ in 0..2 {
        harness.key_press(egui::Key::ArrowDown);
    }
    harness.key_press(egui::Key::ArrowRight);
    picture(&mut harness, "level");
}

/// A query: the hits of the whole tree, flat, with the levels they are in.
#[test]
fn a_query_and_its_hits() {
    let mut harness = harness(opened(entries()), WIDE);
    harness.run();
    harness.event(egui::Event::Text("tty".to_string()));
    picture(&mut harness, "search");
}

/// A wide window stands the plate of a whole beside the menu.
#[test]
fn a_whole_beside_the_menu() {
    let mut harness = harness(opened(cut(4)), WIDE);
    picture(&mut harness, "whole-beside");
}

/// A narrow window stands it above the menu.
#[test]
fn a_whole_above_the_menu() {
    let mut harness = harness(opened(cut(4)), NARROW);
    picture(&mut harness, "whole-above");
}

/// A menu too tall for the plate above it moves down and gives up plates.
#[test]
fn a_whole_above_a_menu_too_tall_for_both() {
    let mut harness = harness(opened(cut(30)), NARROW);
    picture(&mut harness, "whole-above-shortened");
}

/// A narrow window over entries whose wholes are of two heights, the
/// selection on the first, short one.
fn uneven_harness() -> Harness<'static, PlateMenu> {
    harness(opened(uneven(20)), NARROW)
}

/// Under a short whole the menu keeps its plates.
#[test]
fn a_short_whole_above_the_menu() {
    let mut harness = uneven_harness();
    picture(&mut harness, "whole-short");
}

/// One step down, under a tall whole, the menu moves down and gives up plates.
#[test]
fn a_tall_whole_above_the_menu() {
    let mut harness = uneven_harness();
    harness.run();
    harness.key_press(egui::Key::ArrowDown);
    picture(&mut harness, "whole-tall");
}
