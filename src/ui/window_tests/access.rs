//! The window as a screen reader reads it: the widgets whose face carries no
//! text still carry a name.

use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::{NodeT, Queryable};
use rust_i18n::t;

use super::window;

#[test]
fn a_button_of_an_icon_is_named_by_its_hint() {
    let window = window();
    let name = t!("command.search.open");
    let search = window.harness.get_by_label(&name);
    assert_eq!(search.accesskit_node().role(), Role::Button);
}

#[test]
fn a_switch_is_a_checkbox_named_by_its_row() {
    let mut window = window();
    window.harness.get_by_label(&t!("menu.settings")).click();
    window.harness.run_steps(3);

    let name = t!("settings.status_bar");
    let switch = window
        .harness
        .query_all_by_label(&name)
        .find(|node| node.accesskit_node().role() == Role::CheckBox)
        .expect("the switch of the status bar");
    assert_eq!(switch.accesskit_node().toggled(), Some(Toggled::True));
}
