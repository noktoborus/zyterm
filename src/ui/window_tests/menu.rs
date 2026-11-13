//! A click beside the menu: what it chooses, and that it reaches nothing under
//! it.

use egui_kittest::kittest::Queryable;
use egui_plate_menu::MenuItem;
use rust_i18n::t;

use super::{Window, window};
use crate::ui::MainView;

/// The window with a menu whose selection stands on the entry that opens the
/// settings, and where the button of the search stands under the pointer.
fn menu_over_the_search() -> (Window, egui::Pos2) {
    let mut window = window();
    let search = window
        .harness
        .get_by_label(&t!("command.search.open"))
        .rect()
        .center();
    let items = vec![
        MenuItem::new("nothing", "Nothing"),
        MenuItem::new("settings.open", "Settings"),
    ];
    window
        .harness
        .state_mut()
        .menu
        .open_at(items, "settings.open")
        .expect("the menu opens");
    window.harness.run_steps(3);
    (window, search)
}

/// Presses and lets go of `button` at `at`.
fn press(window: &mut Window, at: egui::Pos2, button: egui::PointerButton) {
    let event = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let events = &mut window.harness.input_mut().events;
    events.push(egui::Event::PointerMoved(at));
    events.push(event(true));
    window.harness.step();
    window.harness.input_mut().events.push(event(false));
    window.harness.run_steps(3);
}

#[test]
fn the_left_button_beside_the_menu_chooses_and_presses_nothing_under_it() {
    let (mut window, search) = menu_over_the_search();
    press(&mut window, search, egui::PointerButton::Primary);

    let app = window.harness.state();
    assert!(!app.menu.is_open());
    assert_eq!(app.ui.view, MainView::Settings, "the selected entry ran");
    assert!(
        !app.search.open,
        "the button under the click was not pressed"
    );
}

#[test]
fn the_right_button_beside_the_menu_closes_it_and_presses_nothing_under_it() {
    let (mut window, search) = menu_over_the_search();
    press(&mut window, search, egui::PointerButton::Secondary);

    let app = window.harness.state();
    assert!(!app.menu.is_open());
    assert_ne!(app.ui.view, MainView::Settings, "nothing was chosen");
    assert!(
        !app.search.open,
        "the button under the click was not pressed"
    );
}

#[test]
fn the_right_button_that_opens_the_menu_of_the_terminal_leaves_it_open() {
    let mut window = window();
    press(
        &mut window,
        egui::pos2(500.0, 300.0),
        egui::PointerButton::Secondary,
    );

    assert!(
        window.harness.state().menu.is_open(),
        "the click that opened it is not the one that steps out of it"
    );
}

/// Where the right button lands beside the menu: on the terminal.
const ON_THE_TERMINAL: egui::Pos2 = egui::pos2(60.0, 60.0);

/// A menu of a level, standing inside it.
fn menu_in_a_level(window: &mut Window) {
    let items = vec![
        MenuItem::new("port", "Port").children(vec![
            MenuItem::new("usb0", "ttyUSB0"),
            MenuItem::new("usb1", "ttyUSB1"),
        ]),
        MenuItem::new("quit", "Quit"),
    ];
    window
        .harness
        .state_mut()
        .menu
        .open_at(items, "usb1")
        .expect("the menu opens");
    window.harness.run_steps(3);
}

/// The window with a program on the line that asked for the mouse.
fn reporting_the_mouse() -> Window {
    let mut window = window();
    window.device_says(b"\x1b[?1000h");
    window.step_until("the mouse asked for", |window| {
        window.harness.state_mut().session.page().modes.mouse_report
    });
    window
}

#[test]
fn the_right_button_on_the_terminal_reaches_a_program_that_asked_for_the_mouse() {
    let mut window = reporting_the_mouse();
    press(&mut window, ON_THE_TERMINAL, egui::PointerButton::Secondary);
    window.wait_written("\x1b[M");
}

#[test]
fn the_right_button_beside_the_menu_steps_out_and_reaches_no_program() {
    let mut window = reporting_the_mouse();
    window.written();
    menu_in_a_level(&mut window);

    press(&mut window, ON_THE_TERMINAL, egui::PointerButton::Secondary);
    assert!(window.harness.state().menu.is_open(), "a step out");
    assert!(window.shows("Quit"), "the level above");

    press(&mut window, ON_THE_TERMINAL, egui::PointerButton::Secondary);
    assert!(!window.harness.state().menu.is_open(), "closed at the top");

    window.harness.run_steps(5);
    assert_eq!(window.written(), "", "nothing went to the program");
}

#[test]
fn the_right_button_on_the_menu_reaches_no_program() {
    let mut window = reporting_the_mouse();
    window.written();
    menu_in_a_level(&mut window);
    let entry = window.harness.get_by_label("ttyUSB0").rect().center();

    press(&mut window, entry, egui::PointerButton::Secondary);
    assert!(window.shows("Quit"), "a step out");

    window.harness.run_steps(5);
    assert_eq!(window.written(), "", "nothing went to the program");
}

#[test]
fn the_right_button_beside_the_menu_opens_no_menu_of_the_terminal() {
    let mut window = window();
    menu_in_a_level(&mut window);

    press(&mut window, ON_THE_TERMINAL, egui::PointerButton::Secondary);
    press(&mut window, ON_THE_TERMINAL, egui::PointerButton::Secondary);

    assert!(
        !window.harness.state().menu.is_open(),
        "the click that closed the menu opened no other"
    );
}

#[test]
fn the_left_and_the_middle_button_beside_the_menu_reach_no_program() {
    for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
        let mut window = reporting_the_mouse();
        window.written();
        menu_in_a_level(&mut window);

        press(&mut window, ON_THE_TERMINAL, button);

        window.harness.run_steps(5);
        assert_eq!(
            window.written(),
            "",
            "{button:?}: nothing went to the program"
        );
    }
}
