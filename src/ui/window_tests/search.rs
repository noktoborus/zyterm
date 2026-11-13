//! The search bar as it opens: its whole query selected, the cursor at its
//! end, so typing replaces the query and one Backspace erases it.

use super::{Window, window};

/// Opens the bar by its key and waits until the field has selected its text.
fn open(window: &mut Window) {
    window
        .harness
        .key_press_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::F);
    window.step_until("the bar open, its query settled", |window| {
        let search = &window.harness.state().search;
        search.open && !search.focus_wanted && !search.select_wanted
    });
    window.harness.run_steps(2);
}

fn query(window: &Window) -> &str {
    &window.harness.state().search.query
}

#[test]
fn backspace_erases_the_query_the_bar_opened_with() {
    let mut window = window();
    window.harness.state_mut().search.query = "old".to_string();
    open(&mut window);
    assert_eq!(query(&window), "old", "no selection keeps the query");

    window.harness.key_press(egui::Key::Backspace);
    window.harness.run_steps(2);
    assert_eq!(query(&window), "", "the whole query was selected");
}

#[test]
fn typing_replaces_the_query_the_bar_opened_with() {
    let mut window = window();
    window.harness.state_mut().search.query = "old".to_string();
    open(&mut window);

    window
        .harness
        .input_mut()
        .events
        .push(egui::Event::Text("gamma".to_string()));
    window.harness.run_steps(2);
    assert_eq!(query(&window), "gamma");
}
