//! The block of debugging on the settings page: the debug window moved into
//! it, and the two levels of the journal of messages set apart.

use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use super::{Window, window};
use crate::config::MessageLevel;

/// The window on the settings page, scrolled to its end, where the block is.
fn at_the_block() -> Window {
    let mut window = window();
    window
        .harness
        .state_mut()
        .show_view(crate::ui::MainView::Settings);
    window.harness.run_steps(3);
    let heading = t!("settings.debug");
    window.step_until("the block of debugging", |window| {
        window.harness.input_mut().events.extend([
            egui::Event::PointerMoved(egui::pos2(500.0, 300.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Page,
                delta: egui::vec2(0.0, -4.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        window.shows(&heading)
    });
    window.harness.run_steps(3);
    window
}

#[test]
fn the_debug_window_is_switched_on_in_the_block_of_debugging() {
    let mut window = at_the_block();
    assert!(!window.harness.state().settings.debug.window);
    window
        .harness
        .get_by_role_and_label(
            egui::accesskit::Role::CheckBox,
            &t!("settings.debug_window"),
        )
        .click();
    window.harness.run_steps(3);
    assert!(window.harness.state().settings.debug.window);
}

#[test]
fn the_messages_of_the_libraries_are_set_apart_from_those_of_zyterm() {
    let mut window = at_the_block();
    let off = crate::ui::choice::message_level_label(MessageLevel::Off);
    let buttons: Vec<_> = window.harness.query_all_by_label(&off).collect();
    assert_eq!(buttons.len(), 2, "both levels stand at off");
    buttons[1].click();
    let info = crate::ui::choice::message_level_label(MessageLevel::Info);
    window.step_until("the menu of the levels", |window| window.shows(&info));
    window.harness.get_by_label(&info).click();
    window.step_until("the level of the libraries set", |window| {
        window.harness.state().settings.debug.messages_others == MessageLevel::Info
    });
    assert_eq!(
        window.harness.state().settings.debug.messages,
        MessageLevel::Off,
        "and that of zyterm left as it was"
    );
}
