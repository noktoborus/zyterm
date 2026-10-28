//! The window in tmux control mode: the button, the menu, what a choice
//! writes to the line.

use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use super::{MODES, replies, window};

const WINDOWS: &str = "$0 @0 0 1 0 %0 sh";
const PANES: &str = "$0 @0 %0 0 0 sh\n$0 @0 %1 0 1 bash";
const TWO_PANES: &str = "8205,80x24,0,0{40x24,0,0,0,39x24,41,0,1}";

#[test]
fn a_pane_of_a_window_of_two_is_chosen_from_the_menu_of_tmux() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229e} 0.0 sh")
    });

    window.open_tmux_menu("\u{229e} 0.0 sh", WINDOWS, PANES);
    assert!(window.shows("0: sh"), "the window, in the menu");

    window.harness.key_press(egui::Key::ArrowRight);
    window.harness.run_steps(3);
    assert!(window.shows("0.1 bash"), "its panes one level in");
    assert!(window.shows("0.0 sh"));
    window.harness.key_press(egui::Key::ArrowDown);
    window.harness.key_press(egui::Key::Enter);
    let sent = window.wait_written("capture-pane");
    assert!(sent.contains("display -p -t %1 "), "{sent}");
    assert!(
        !sent.contains("select-pane"),
        "nothing sent for the pane: {sent}"
    );
    window.step_until("the pane chosen on the button at once", |window| {
        window.shows("\u{229e} 0.1 bash")
    });
    let plate = t!("tmux.loading_screen");
    window.step_until("the plate over the pane as it is", |window| {
        window.shows(&plate)
    });

    window.device_says(&replies(20, &[MODES, "", "$ bash"]));
    window.step_until("the plate gone with the snapshot", |window| {
        !window.shows(&plate)
    });
}

#[test]
fn a_window_is_split_and_a_pane_zoomed_from_the_menu_of_tmux() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229e} 0.0 sh")
    });
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window.written();
    window.open_tmux_menu("\u{229e} 0.0 sh", WINDOWS, PANES);
    window.harness.key_press(egui::Key::ArrowRight);
    window.harness.run_steps(3);
    assert!(window.shows(&t!("tmux.split_side_by_side")));
    assert!(window.shows(&t!("tmux.split_one_above")));
    window
        .harness
        .get_by_label(&t!("tmux.split_side_by_side"))
        .click();
    let sent = window.wait_written("split-window");
    assert!(sent.contains("split-window -h -t %0"), "{sent}");
    window.device_says(&replies(90, &[""]));

    window.open_tmux_menu("\u{229e} 0.0 sh", WINDOWS, PANES);
    for key in [
        egui::Key::ArrowRight,
        egui::Key::ArrowDown,
        egui::Key::ArrowRight,
    ] {
        window.harness.key_press(key);
        window.harness.run_steps(3);
    }
    assert!(window.shows(&t!("tmux.close_named", window = "0.1 bash")));
    window.harness.get_by_label(&t!("tmux.zoom")).click();
    let sent = window.wait_written("resize-pane");
    assert!(sent.contains("resize-pane -Z -t %1"), "{sent}");
}

#[test]
fn the_panes_stand_side_by_side_the_divider_drags_and_a_press_takes_the_focus() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    let divider = t!("tmux.divider");
    window.step_until("the divider", |window| window.shows(&divider));
    window.written();

    let at = window.harness.get_by_label(&divider).rect().center();
    let to = at - egui::vec2(80.0, 0.0);
    for (event, pos) in [("press", at), ("move", to), ("release", to)] {
        window.harness.event(match event {
            "move" => egui::Event::PointerMoved(pos),
            _ => egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: event == "press",
                modifiers: egui::Modifiers::NONE,
            },
        });
        window.harness.event(egui::Event::PointerMoved(pos));
        window.harness.run_steps(2);
    }
    let sent = window.wait_written("resize-pane");
    assert!(sent.contains("resize-pane -t %0 -x "), "{sent}");
    window.device_says(&replies(90, &[""]));

    let beside = at + egui::vec2(120.0, 0.0);
    window.harness.event(egui::Event::PointerMoved(beside));
    window.harness.event(egui::Event::PointerButton {
        pos: beside,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    window.harness.event(egui::Event::PointerButton {
        pos: beside,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    window.step_until("the pane beside in focus", |window| {
        window.shows("\u{229e} 0.1 bash")
    });
    assert!(
        !window.written().contains("select-pane"),
        "the focus is this side's alone"
    );
}

/// The press that moves the focus is held and dragged in the pane it moved
/// to: the pane that had the focus, now drawn beside, takes no part of it.
#[test]
fn a_drag_after_the_press_that_moves_the_focus_selects_nothing_in_the_pane_left() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    let divider = t!("tmux.divider");
    window.step_until("the divider", |window| window.shows(&divider));
    window.written();

    let beside = window.harness.get_by_label(&divider).rect().center() + egui::vec2(120.0, 0.0);
    window.harness.event(egui::Event::PointerMoved(beside));
    window.harness.event(egui::Event::PointerButton {
        pos: beside,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    window.step_until("the pane beside in focus", |window| {
        window.shows("\u{229e} 0.1 bash")
    });
    for step in 1..=4 {
        let to = beside + egui::vec2(20.0 * step as f32, 0.0);
        window.harness.event(egui::Event::PointerMoved(to));
        window.harness.run_steps(2);
    }
    window.harness.event(egui::Event::PointerButton {
        pos: beside + egui::vec2(80.0, 0.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    window.harness.run_steps(10);
    let left = window
        .harness
        .state()
        .session
        .beside(zyt_tmux::PaneId(0))
        .map(|snapshot| snapshot.page().selecting);
    assert_eq!(left, Some(false), "no selection in the pane left");
}

#[test]
fn the_wheel_over_a_pane_beside_reaches_its_program_and_leaves_the_focus() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ htop"]));
    window.device_says(b"%output %1 \\033[?1049h\\033[?1000h\\033[?1006h\r\n");
    let divider = t!("tmux.divider");
    window.step_until("the divider", |window| window.shows(&divider));
    window.harness.run_steps(3);
    window.written();

    let beside = window.harness.get_by_label(&divider).rect().center() + egui::vec2(120.0, 0.0);
    window.harness.event(egui::Event::PointerMoved(beside));
    window.harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    let sent = window.wait_written("send-keys");
    assert!(sent.contains("send-keys -t %1 -H 1b 5b 3c 36 35"), "{sent}");
    window.harness.run_steps(3);
    assert!(window.shows("\u{229e} 0.0 sh"), "the focus stays");
}

/// A window of panes is chosen by its label, so its panes are reached by the
/// zone at the right end of its plate, which a pointer can press.
#[test]
fn the_panes_of_a_window_are_reached_by_the_pointer() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229e} 0.0 sh")
    });
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window.written();
    window.open_tmux_menu("\u{229e} 0.0 sh", WINDOWS, PANES);

    let row = window.harness.get_by_label("0: sh").rect().center().y;
    let into = t!("menu.step_in");
    window
        .harness
        .query_all_by_label(&into)
        .find(|zone| (zone.rect().center().y - row).abs() < 1.0)
        .expect("the window of two panes has a zone")
        .click();
    window.harness.run_steps(3);

    assert!(window.shows("0.1 bash"), "its panes one level in");
    assert!(window.shows("0.0 sh"));
    assert!(window.written().is_empty(), "nothing chosen");
}

#[test]
fn a_bell_of_a_hidden_window_is_counted_on_the_button() {
    let mut window = window();
    window.in_tmux(
        "$0 @0 0 1 0 %0 sh\n$0 @1 1 0 0 %1 vim",
        "$0 @0 %0 0 0 sh\n$0 @1 %1 1 0 vim",
        "$ ls",
    );
    window.device_says(b"%output %1 \\007\r\n");
    window.step_until("the bell counted on the button", |window| {
        window.shows("\u{229e} 0: sh  \u{1f514}1")
    });
}

#[test]
fn the_wheel_over_the_button_walks_the_windows_and_tells_tmux_where_it_rests() {
    let mut window = window();
    window.in_tmux(
        "$0 @0 0 1 0 %0 sh\n$0 @1 1 0 0 %1 vim",
        "$0 @0 %0 0 0 sh\n$0 @1 %1 1 0 vim",
        "$ ls",
    );
    window.step_until("the button", |window| window.shows("\u{229e} 0: sh"));
    window.written();
    let button = window
        .harness
        .get_by_label("\u{229e} 0: sh")
        .rect()
        .center();
    window.harness.event(egui::Event::PointerMoved(button));
    window.harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    window.step_until("the next window on the button at once", |window| {
        window.shows("\u{229e} 1: vim")
    });
    let sent = window.wait_written("select-window");
    assert!(sent.contains("select-window -t @1"), "{sent}");
}

#[test]
fn rx_stands_while_the_screen_is_behind_and_skips_what_piled_up() {
    let mut window = window();
    window.harness.state_mut().settings.tmux.pause = true;
    window.harness.run_steps(2);
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(b"%extended-output %0 14000 : x\r\n");
    window.step_until("RX on the bar", |window| window.shows("RX \u{2212}14 s"));
    window.written();
    window.harness.get_by_label("RX \u{2212}14 s").click();
    window.wait_written("refresh-client -A '%0:pause'");
}

#[test]
fn row_minus_one_offers_the_history_of_tmux_and_a_press_asks_for_it() {
    let mut window = window();
    let modes = "80 24 50 0 4294967295 4294967295 2 0 0 23 0 1 0 0 0 1 0 0 0 0 0 8,16";
    window.in_tmux_with(modes, WINDOWS, PANES, "$ ls");
    window.step_until("the history offered", |window| {
        window.harness.state().session.tmux_above_top().is_some()
    });
    window
        .harness
        .event(egui::Event::PointerMoved(egui::pos2(300.0, 200.0)));
    window.harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    window.harness.run_steps(3);
    window.written();
    let top = egui::pos2(300.0, 6.0);
    window.harness.event(egui::Event::PointerMoved(top));
    for pressed in [true, false] {
        window.harness.event(egui::Event::PointerButton {
            pos: top,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    let asked = window.wait_written("display -p -t %0 ");
    assert!(!asked.contains("capture-pane"), "the modes first: {asked}");
}
