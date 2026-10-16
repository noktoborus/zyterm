//! The menu as accesskit sees it: what a screen reader reads and a test of a
//! window that holds the menu finds.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use plate_menu::{MenuItem, PlateMenu};

fn harness(items: Vec<MenuItem>) -> Harness<'static, PlateMenu> {
    let mut menu = PlateMenu::new();
    menu.open(items).expect("an entry can be chosen");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            |ui, menu: &mut PlateMenu| {
                menu.show(ui.ctx());
            },
            menu,
        );
    harness.run();
    harness
}

#[test]
fn a_plate_is_a_button_named_by_its_label() {
    let harness = harness(vec![
        MenuItem::new("copy", "Copy").detail("ctrl+shift+c"),
        MenuItem::new("send", "Send a file").enabled(false),
    ]);
    let copy = harness.get_by_label("Copy");
    assert_eq!(copy.accesskit_node().role(), egui::accesskit::Role::Button);
    assert_eq!(
        copy.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the selection"
    );
    assert!(
        harness
            .get_by_label("Send a file")
            .accesskit_node()
            .is_disabled(),
        "an entry that cannot be reached"
    );
}

/// Entries that are chosen and carry entries of their own, beside one that
/// only carries them.
fn sources() -> Vec<MenuItem> {
    vec![
        MenuItem::new("usb0", "/dev/ttyUSB0")
            .choosable(true)
            .children(vec![MenuItem::new("usb0.default", "At start")]),
        MenuItem::new("theme", "Theme").children(vec![MenuItem::new("dark", "Dark")]),
    ]
}

fn named(mut harness: Harness<'static, PlateMenu>) -> Harness<'static, PlateMenu> {
    harness.state_mut().step_in_label("Open the entries inside");
    harness.run();
    harness
}

/// An entry that is chosen and carries entries is two buttons: its label, and
/// the zone that steps in, named by the caller. An entry that only carries
/// entries is one.
#[test]
fn an_entry_that_is_chosen_has_a_button_that_steps_in() {
    let harness = named(harness(sources()));
    let zones = harness
        .query_all_by_label("Open the entries inside")
        .count();
    assert_eq!(zones, 1, "Theme has no zone of its own");
    assert_eq!(
        harness
            .get_by_label("Open the entries inside")
            .accesskit_node()
            .role(),
        egui::accesskit::Role::Button
    );
}

/// A click on the zone steps in, the way `Right` does.
#[test]
fn a_click_on_the_zone_steps_in() {
    let mut harness = named(harness(sources()));
    harness.get_by_label("Open the entries inside").click();
    harness.run();

    assert!(harness.state().is_open());
    harness.get_by_label("At start");
    harness.get_by_label("\u{2039} /dev/ttyUSB0");
}

/// A click on the label chooses the entry, and the menu closes.
#[test]
fn a_click_on_the_label_chooses() {
    let mut harness = named(harness(sources()));
    harness.get_by_label("/dev/ttyUSB0").click();
    harness.run();

    assert!(!harness.state().is_open());
}

/// A click on an entry that only carries entries steps in from anywhere on it.
#[test]
fn a_click_on_an_entry_that_only_carries_entries_steps_in() {
    let mut harness = named(harness(sources()));
    harness.get_by_label("Theme").click();
    harness.run();

    assert!(harness.state().is_open());
    harness.get_by_label("Dark");
}

/// A query that finds nothing is answered by the caller's line.
#[test]
fn a_query_that_finds_nothing_says_so() {
    let mut harness = harness(sources());
    harness.state_mut().nothing_found("Nothing found");
    harness.event(egui::Event::Text("zzz".to_string()));
    harness.run();

    harness.get_by_label("Nothing found");
    assert!(harness.query_by_label("Theme").is_none());
}
