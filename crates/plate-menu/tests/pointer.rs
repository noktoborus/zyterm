//! The pointer and the finger: what moving onto a plate does, and what a
//! click or a touch beside the menu does.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use plate_menu::{Beside, Chosen, MenuItem, PlateMenu};

/// Where a click lands beside the menu, which stands in the middle.
const BESIDE: egui::Pos2 = egui::pos2(5.0, 5.0);

/// The menu and what it chose last.
struct Shown {
    menu: PlateMenu,
    chosen: Option<Chosen>,
}

fn harness(beside: Beside) -> Harness<'static, Shown> {
    opened(
        vec![
            MenuItem::new("one", "One"),
            MenuItem::new("two", "Two"),
            MenuItem::new("three", "Three"),
            MenuItem::new("four", "Four"),
        ],
        None,
        beside,
    )
}

/// A menu of a level, opened inside it.
fn in_a_level(beside: Beside) -> Harness<'static, Shown> {
    let items = vec![
        MenuItem::new("port", "Port").children(vec![
            MenuItem::new("usb0", "ttyUSB0"),
            MenuItem::new("usb1", "ttyUSB1"),
        ]),
        MenuItem::new("quit", "Quit"),
    ];
    opened(items, Some("usb1"), beside)
}

fn opened(items: Vec<MenuItem>, at: Option<&str>, beside: Beside) -> Harness<'static, Shown> {
    let mut menu = PlateMenu::new();
    match at {
        Some(id) => menu.open_at(items, id),
        None => menu.open(items),
    }
    .expect("an entry can be chosen");
    menu.beside(beside);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            |ui, shown: &mut Shown| {
                if let Some(chosen) = shown.menu.show(ui.ctx()) {
                    shown.chosen = Some(chosen);
                }
            },
            Shown { menu, chosen: None },
        );
    harness.run();
    harness
}

/// Presses and lets go of a button beside the menu, by a finger when `finger`
/// is set: the toolkit is handed a touch and the press of the left button it
/// makes of one, the way the system hands it over.
fn press_beside(harness: &mut Harness<'static, Shown>, button: egui::PointerButton, finger: bool) {
    let touch = |phase| egui::Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId(0),
        phase,
        pos: BESIDE,
        force: None,
    };
    let press = |pressed| egui::Event::PointerButton {
        pos: BESIDE,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };

    let events = &mut harness.input_mut().events;
    events.push(egui::Event::PointerMoved(BESIDE));
    if finger {
        events.push(touch(egui::TouchPhase::Start));
    }
    events.push(press(true));
    harness.step();
    let events = &mut harness.input_mut().events;
    if finger {
        events.push(touch(egui::TouchPhase::End));
    }
    events.push(press(false));
    harness.step();
}

#[test]
fn the_pointer_moved_onto_a_plate_selects_it() {
    let mut harness = harness(Beside::Answers);
    harness.get_by_label("Three").hover();
    harness.run();
    assert_eq!(harness.state().menu.selected(), Some(2));

    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    assert_eq!(
        harness.state().menu.selected(),
        Some(3),
        "a pointer that stands still does not take the selection back"
    );
}

#[test]
fn the_left_button_beside_the_menu_chooses_the_selected_entry() {
    let mut harness = harness(Beside::Answers);
    harness.key_press(egui::Key::ArrowDown);
    harness.run();

    press_beside(&mut harness, egui::PointerButton::Primary, false);

    let chosen = harness
        .state()
        .chosen
        .as_ref()
        .map(|chosen| chosen.id.as_str());
    assert_eq!(chosen, Some("two"));
    assert!(!harness.state().menu.is_open());
}

#[test]
fn a_finger_and_the_other_buttons_beside_the_menu_at_the_top_close_it() {
    for (button, finger) in [
        (egui::PointerButton::Primary, true),
        (egui::PointerButton::Secondary, false),
        (egui::PointerButton::Middle, false),
    ] {
        let mut harness = harness(Beside::Answers);
        press_beside(&mut harness, button, finger);

        assert!(
            harness.state().chosen.is_none(),
            "{button:?}, finger {finger}: nothing chosen"
        );
        assert!(
            !harness.state().menu.is_open(),
            "{button:?}, finger {finger}: closed"
        );
    }
}

#[test]
fn nothing_beside_a_menu_that_ignores_it_leaves_it() {
    for (button, finger) in [
        (egui::PointerButton::Primary, false),
        (egui::PointerButton::Primary, true),
        (egui::PointerButton::Secondary, false),
    ] {
        let mut harness = harness(Beside::Ignored);
        press_beside(&mut harness, button, finger);

        assert!(harness.state().chosen.is_none());
        assert!(
            harness.state().menu.is_open(),
            "{button:?}, finger {finger}: open"
        );
    }
}

#[test]
fn the_right_button_steps_out_of_a_level_on_the_menu_and_beside_it() {
    let mut harness = in_a_level(Beside::Answers);
    harness
        .get_by_label("ttyUSB0")
        .click_button(egui::PointerButton::Secondary);
    harness.run();
    assert!(harness.state().menu.is_open(), "on the menu: a step out");
    assert!(harness.query_by_label("Quit").is_some(), "the top level");

    let mut harness = in_a_level(Beside::Answers);
    press_beside(&mut harness, egui::PointerButton::Secondary, false);
    harness.run();
    assert!(harness.state().menu.is_open(), "beside it: a step out");
    assert!(harness.query_by_label("Quit").is_some(), "the top level");
    assert!(harness.state().chosen.is_none());
}

#[test]
fn the_right_button_on_the_menu_at_the_top_closes_it() {
    let mut harness = harness(Beside::Answers);
    harness
        .get_by_label("Two")
        .click_button(egui::PointerButton::Secondary);
    harness.run();
    assert!(!harness.state().menu.is_open());
    assert!(harness.state().chosen.is_none(), "nothing chosen");
}

#[test]
fn the_right_button_steps_out_of_a_menu_that_ignores_a_click_but_never_closes_it() {
    let mut harness = in_a_level(Beside::Ignored);
    harness
        .get_by_label("ttyUSB0")
        .click_button(egui::PointerButton::Secondary);
    harness.run();
    assert!(harness.query_by_label("Quit").is_some(), "a step out");

    harness
        .get_by_label("Quit")
        .click_button(egui::PointerButton::Secondary);
    harness.run();
    assert!(harness.state().menu.is_open(), "not closed at the top");
}
