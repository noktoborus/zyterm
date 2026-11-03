//! The window a script asks with: how wide its values grow.

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use rust_i18n::t;
use zyt_script::{Field, FieldKind, Form};

use super::window;

/// The width of the harness, which is the room a form has.
const ROOM: f32 = 1000.0;

/// A form of one line and one block, opened with these values in them.
fn asked(line: &str, block: &str) -> super::Window {
    opened(
        Form::new("Ask")
            .and(Field::new(
                "line",
                FieldKind::Text {
                    value: line.to_string(),
                    password: false,
                },
            ))
            .and(Field::new(
                "block",
                FieldKind::Textarea {
                    value: block.to_string(),
                    rows: 2,
                },
            )),
    )
}

/// A form of a block of this many lines and a switch under it.
fn block_of(lines: usize) -> super::Window {
    let text: Vec<String> = (1..=lines).map(|line| format!("line {line}")).collect();
    opened(
        Form::new("Ask")
            .and(Field::new(
                "block",
                FieldKind::Textarea {
                    value: text.join("\n"),
                    rows: 2,
                },
            ))
            .and(Field::new("switch", FieldKind::Switch { value: true })),
    )
}

/// The window with this form standing in it.
fn opened(form: Form) -> super::Window {
    let mut window = window();
    let pending = crate::ui::tests::pending(form);
    window.harness.state_mut().ui.form = Some(crate::ui::form::State::new(
        pending,
        None,
        String::new(),
        String::new(),
        None,
        false,
    ));
    window.harness.run_steps(4);
    window
}

/// Where the named widget of one of these roles stands, as the tree of the
/// window gives it; its label carries the same name.
fn rect_of(window: &super::Window, name: &str, roles: &[Role]) -> egui::Rect {
    window
        .harness
        .query_all_by_label(name)
        .find(|node| roles.contains(&node.accesskit_node().role()))
        .expect("the widget")
        .rect()
}

/// The width of the named field.
fn width_of(window: &super::Window, name: &str) -> f32 {
    rect_of(window, name, &[Role::TextInput, Role::MultilineTextInput]).width()
}

/// Where the switch under a block stands.
fn switch_of(window: &super::Window) -> egui::Rect {
    rect_of(window, "switch", &[Role::CheckBox])
}

#[test]
fn a_short_value_keeps_the_window_narrow() {
    let window = asked("abc", "one\ntwo");

    assert!(width_of(&window, "line") < ROOM / 3.0);
    assert!(width_of(&window, "block") < ROOM / 3.0);
}

#[test]
fn a_value_grows_past_sixty_four_characters_while_the_room_holds_it() {
    let long = "x".repeat(80);
    let short = asked("abc", "abc");
    let window = asked(&long, &format!("{long}\nshort"));
    let go_on = window.harness.get_by_label(&t!("dialog.ok")).rect();

    for name in ["line", "block"] {
        assert!(
            width_of(&window, name) > width_of(&short, name) * 2.5,
            "{name} grew with what is written in it"
        );
    }
    assert!(go_on.right() <= ROOM, "the window stands inside the room");
}

#[test]
fn a_block_wider_than_the_room_scrolls_and_the_window_does_not_grow() {
    let wide = "y".repeat(300);
    let window = asked(&wide, &format!("{wide}\nshort"));
    let go_on = window.harness.get_by_label(&t!("dialog.ok")).rect();

    assert!(go_on.right() <= ROOM, "the window stands inside the room");
    assert!(
        width_of(&window, "line") < ROOM,
        "a line follows its caret inside the room"
    );
    assert!(
        width_of(&window, "block") > ROOM,
        "the block is as wide as its longest line, scrolled sideways"
    );
}

#[test]
fn a_block_of_few_lines_is_five_lines_tall_and_the_row_under_it_stands_below() {
    let one = block_of(1);
    let five = block_of(5);
    let block = rect_of(&one, "block", &[Role::MultilineTextInput]);

    assert_eq!(switch_of(&one), switch_of(&five), "one line stands as five");
    assert!(
        switch_of(&one).top() >= block.bottom(),
        "the switch {:?} stands below the block {block:?}",
        switch_of(&one)
    );
}

#[test]
fn a_block_past_ten_lines_scrolls_inside_and_the_window_stays() {
    let seven = block_of(7);
    let ten = block_of(10);
    let thirty = block_of(30);
    let go_on = |window: &super::Window| window.harness.get_by_label(&t!("dialog.ok")).rect();
    let near = |one: egui::Rect, two: egui::Rect| (one.top() - two.top()).abs() < 1.0;

    assert!(
        !near(switch_of(&seven), switch_of(&ten)),
        "it grows up to ten"
    );
    assert!(near(switch_of(&ten), switch_of(&thirty)), "and no further");
    assert!(
        near(go_on(&ten), go_on(&thirty)),
        "the window stays as it was"
    );
}
