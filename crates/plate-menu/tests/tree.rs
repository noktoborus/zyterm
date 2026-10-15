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
