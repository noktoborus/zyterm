//! The window in tmux control mode: the button, the menu, what a choice
//! writes to the line.

use egui_kittest::kittest::{NodeT, Queryable};
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
        window.shows("\u{229f} 0.0 sh")
    });

    window.open_tmux_menu("\u{229f} 0.0 sh", WINDOWS, PANES);
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
        window.shows("\u{229f} 0.1 bash")
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
fn the_pane_shown_is_split_zoomed_and_moved_out_from_the_top_of_the_menu() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229f} 0.0 sh")
    });
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window.written();
    for (entry, command) in [
        ("tmux.split_side_by_side", "split-window -h -t %0"),
        ("tmux.zoom", "resize-pane -Z -t %0"),
        ("tmux.break_pane", "break-pane -s %0"),
    ] {
        window.open_tmux_menu("\u{229f} 0.0 sh", WINDOWS, PANES);
        assert!(
            window.shows(&t!("tmux.split_one_above")),
            "at the top level"
        );
        window.harness.get_by_label(&t!(entry)).click();
        let sent = window.wait_written(command);
        assert!(sent.contains(command), "{entry}: {sent}");
        window.device_says(&replies(90, &[""]));
        window.harness.run_steps(3);
    }
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
        window.shows("\u{229f} 0.1 bash")
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
        window.shows("\u{229f} 0.1 bash")
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

/// With the focus following the mouse, the pointer moved over a pane beside
/// gives it the focus with no press; off, the same motion leaves it.
#[test]
fn the_focus_follows_the_pointer_only_when_the_setting_says() {
    for follows in [false, true] {
        let mut window = window();
        window.harness.state_mut().settings.tmux.focus_follows_mouse = follows;
        window.in_tmux(WINDOWS, PANES, "$ ls");
        window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
        window.wait_written("capture-pane");
        window.device_says(&replies(80, &[MODES, "", "$ bash"]));
        let divider = t!("tmux.divider");
        window.step_until("the divider", |window| window.shows(&divider));

        let at = window.harness.get_by_label(&divider).rect().center();
        for step in 1..=3 {
            let to = at + egui::vec2(100.0 + 10.0 * step as f32, 0.0);
            window.harness.event(egui::Event::PointerMoved(to));
            window.harness.run_steps(2);
        }
        window.harness.run_steps(3);
        let label = match follows {
            true => "\u{229f} 0.1 bash",
            false => "\u{229f} 0.0 sh",
        };
        assert!(window.shows(label), "follows {follows}: {label}");
    }
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
    assert!(window.shows("\u{229f} 0.0 sh"), "the focus stays");
}

/// A window of panes is chosen by its label, so its panes are reached by the
/// zone at the right end of its plate, which a pointer can press.
#[test]
fn the_panes_of_a_window_are_reached_by_the_pointer() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229f} 0.0 sh")
    });
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window.written();
    window.open_tmux_menu("\u{229f} 0.0 sh", WINDOWS, PANES);

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
        window.shows("\u{1f5d6} 0: sh  \u{1f514}1")
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
    window.step_until("the button", |window| window.shows("\u{1f5d6} 0: sh"));
    window.written();
    let button = window
        .harness
        .get_by_label("\u{1f5d6} 0: sh")
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
        window.shows("\u{1f5d6} 1: vim")
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

/// tmux silent past `tmux.unanswered_after`: the button says for how long and
/// what is held, the panes are framed, and a press leaves with `detach` alone.
#[test]
fn a_silent_tmux_is_shown_on_the_button_and_left_with_detach() {
    let mut window = window();
    window.harness.state_mut().settings.tmux.unanswered_after = 1;
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.harness.state_mut().send_input(b"x");
    window.wait_written("send-keys");
    window.harness.state_mut().send_input(b"yz");
    window.step_until("the button of a silent tmux", |window| {
        window
            .harness
            .query_by_label_contains("\u{229e} \u{26a0}")
            .is_some()
    });
    assert!(
        window.harness.query_by_label_contains(" · 2").is_some(),
        "the bytes held on the button"
    );
    if let Ok(dir) = std::env::var("ZYTERM_SHOT_DIR") {
        window.harness.run_steps(2);
        let image = window.harness.render().expect("the window renders");
        image
            .save(std::path::Path::new(&dir).join("silent-tmux.png"))
            .expect("the picture is written");
    }

    window
        .harness
        .query_by_label_contains("\u{229e} \u{26a0}")
        .expect("the button")
        .click();
    let sent = window.wait_written("detach");
    assert!(
        !sent.contains("yz"),
        "what was typed is thrown away: {sent}"
    );
    window.step_until("the plain terminal", |window| {
        window.harness.state().session.tmux_look().is_none()
    });
}

/// A pane that takes the focus and gives it back is painted anew beside: the
/// picture of it from before the focus would show its text as it stood then,
/// and its scrollbar where its view is now.
#[test]
fn a_pane_given_the_focus_and_back_is_painted_anew_beside() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    let right = zyt_tmux::PaneId(1);
    window.step_until("a picture of the pane beside", |window| {
        window.harness.state().beside_caches.contains_key(&right)
    });

    let focus = |pane| crate::tmux::Choice::Focus {
        window: zyt_tmux::WindowId(0),
        pane: zyt_tmux::PaneId(pane),
    };
    window.harness.state_mut().session.tmux_choose(focus(1));
    window.step_until("the right pane in focus", |window| {
        window.shows("\u{229f} 0.1 bash")
    });
    assert!(
        !window.harness.state().beside_caches.contains_key(&right),
        "no picture of it kept while it is in focus"
    );

    window.harness.state_mut().session.tmux_choose(focus(0));
    window.step_until("the left pane in focus again", |window| {
        window.shows("\u{229f} 0.0 sh")
    });
    window.step_until("a new picture of the pane beside", |window| {
        window.harness.state().beside_caches.contains_key(&right)
    });
}

/// The cell of a divider wears the background of the terminal, as the panes
/// either side of it, and not the darker one of the window.
#[test]
fn the_cell_of_a_divider_is_the_background_of_the_terminal() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    let divider = t!("tmux.divider");
    window.step_until("the divider", |window| window.shows(&divider));
    window.harness.run_steps(2);

    let rect = window.harness.get_by_label(&divider).rect();
    let image = window.harness.render().expect("the window renders");
    let pixel = image.get_pixel((rect.min.x + 1.0) as u32, rect.center().y as u32);
    let background = window.harness.state().terminal_theme.background;
    assert_eq!(
        [pixel[0], pixel[1], pixel[2]],
        [background.r(), background.g(), background.b()]
    );
}

/// A divider held by the mouse is drawn bright, the line and not its cell, so
/// what is being dragged is seen; let go, it is as dim as before.
#[test]
fn a_divider_held_by_the_mouse_is_drawn_bright() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    let divider = t!("tmux.divider");
    window.step_until("the divider", |window| window.shows(&divider));
    window.harness.run_steps(2);

    let rect = window.harness.get_by_label(&divider).rect();
    let brightness = |window: &mut super::Window, at: egui::Pos2| {
        let image = window.harness.render().expect("the window renders");
        let line = image.get_pixel(at.x as u32, rect.center().y as u32);
        let cell = image.get_pixel((rect.min.x + 1.0) as u32, rect.center().y as u32);
        (
            u32::from(line[0]) + u32::from(line[1]) + u32::from(line[2]),
            [cell[0], cell[1], cell[2]],
        )
    };
    let (resting, cell) = brightness(&mut window, rect.center());

    window
        .harness
        .event(egui::Event::PointerMoved(rect.center()));
    window.harness.event(egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    window.harness.run_steps(2);
    let held = rect.center() + egui::vec2(2.0, 0.0);
    window.harness.event(egui::Event::PointerMoved(held));
    window.harness.run_steps(2);
    let (dragged, held_cell) = brightness(&mut window, rect.center());
    assert!(dragged > resting, "the line held: {dragged} over {resting}");
    assert_eq!(held_cell, cell, "the cell keeps its background");

    window.harness.event(egui::Event::PointerButton {
        pos: held,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    window.harness.run_steps(2);
    let (let_go, _) = brightness(&mut window, rect.center());
    assert_eq!(let_go, resting, "let go, as dim as before");
}

/// The panes of the window shown open on the pane in focus, the one that
/// wears the mark, whichever of them it is.
#[test]
fn the_panes_of_the_menu_open_on_the_pane_in_focus() {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window
        .harness
        .state_mut()
        .session
        .tmux_choose(crate::tmux::Choice::Focus {
            window: zyt_tmux::WindowId(0),
            pane: zyt_tmux::PaneId(1),
        });
    window.step_until("the right pane in focus", |window| {
        window.shows("\u{229f} 0.1 bash")
    });

    window.open_tmux_menu("\u{229f} 0.1 bash", WINDOWS, PANES);
    window.harness.key_press(egui::Key::ArrowRight);
    window.harness.run_steps(3);
    let selected = |window: &super::Window, label: &str| {
        window
            .harness
            .get_by_label(label)
            .accesskit_node()
            .toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    assert!(selected(&window, "0.1 bash"), "the pane in focus");
    assert!(!selected(&window, "0.0 sh"));
}

const CTRL: egui::Modifiers = egui::Modifiers::CTRL;

/// A key at the place of `code` and the text it typed, in one frame, as the
/// toolkit gives them; `Harness::event` would run a frame for each.
fn typed(window: &mut super::Window, code: egui::Key, shift: bool, text: &str) {
    let modifiers = match shift {
        true => egui::Modifiers::SHIFT,
        false => egui::Modifiers::NONE,
    };
    let events = &mut window.harness.input_mut().events;
    events.push(egui::Event::Key {
        key: code,
        physical_key: Some(code),
        pressed: true,
        repeat: false,
        modifiers,
    });
    events.push(egui::Event::Text(text.to_string()));
    window.harness.run_steps(2);
}

fn prefix(window: &mut super::Window) {
    window.harness.key_press_modifiers(CTRL, egui::Key::B);
    window.harness.run_steps(2);
}

/// The window in the mode, two panes side by side, the pane `%0` in focus.
fn two_panes() -> super::Window {
    let mut window = window();
    window.in_tmux(WINDOWS, PANES, "$ ls");
    window.device_says(format!("%layout-change @0 {TWO_PANES} {TWO_PANES} *\r\n").as_bytes());
    window.wait_written("capture-pane");
    window.device_says(&replies(80, &[MODES, "", "$ bash"]));
    window.step_until("the button of the pane shown", |window| {
        window.shows("\u{229f} 0.0 sh")
    });
    window.written();
    window
}

#[test]
fn the_keys_of_tmux_send_what_they_say_with_our_pane_as_the_target() {
    for (key, shift, text, command) in [
        (egui::Key::C, false, "c", "new-window"),
        (egui::Key::Num5, true, "%", "split-window -h -t %0"),
        (egui::Key::Quote, true, "\"", "split-window -v -t %0"),
        (egui::Key::Z, false, "z", "resize-pane -Z -t %0"),
    ] {
        let mut window = two_panes();
        prefix(&mut window);
        typed(&mut window, key, shift, text);
        let sent = window.wait_written(command);
        assert!(sent.contains(command), "{sent}");
    }
}

#[test]
fn the_keys_of_tmux_stand_at_the_same_place_on_a_russian_layout() {
    let mut window = two_panes();
    prefix(&mut window);
    typed(&mut window, egui::Key::C, false, "\u{441}");
    let sent = window.wait_written("new-window");
    assert!(!sent.contains("send-keys"), "{sent}");

    let mut window = two_panes();
    prefix(&mut window);
    typed(&mut window, egui::Key::OpenBracket, true, "\u{425}");
    let sent = window.wait_written("swap-pane");
    assert!(sent.contains("swap-pane -U -t %0"), "{sent}");
    assert!(!sent.contains("send-keys"), "{sent}");
}

#[test]
fn the_prefix_goes_to_the_line_outside_the_mode_and_with_the_setting_off() {
    let mut window = window();
    prefix(&mut window);
    window.wait_written("\u{2}");

    let mut window = two_panes();
    window.harness.state_mut().settings.tmux.keys = false;
    window.harness.run_steps(2);
    prefix(&mut window);
    let sent = window.wait_written("send-keys");
    assert!(sent.contains("send-keys"), "{sent}");
    assert!(
        !window.shows(&t!("tmux.keys.windows")),
        "no plate with the setting off"
    );
}

#[test]
fn the_prefix_twice_sends_the_prefix_to_the_pane() {
    let mut window = two_panes();
    prefix(&mut window);
    prefix(&mut window);
    let sent = window.wait_written("send-keys");
    assert!(sent.contains("-t %0"), "{sent}");
    assert!(!window.shows(&t!("tmux.keys.windows")));
}

#[test]
fn a_key_that_repeats_runs_again_by_itself_and_a_pause_ends_it() {
    let mut window = two_panes();
    window.harness.state_mut().settings.tmux.repeat_time = 5000;
    window.harness.run_steps(2);
    prefix(&mut window);
    for _ in 0..2 {
        window
            .harness
            .key_press_modifiers(CTRL, egui::Key::ArrowLeft);
        window.harness.run_steps(2);
        let sent = window.wait_written("resize-pane");
        assert!(sent.contains("resize-pane -t %0 -L 1"), "{sent}");
        window.device_says(&replies(90, &[""]));
    }
    assert!(
        window.shows(&t!("tmux.keys.resize")),
        "the plate of what repeats"
    );

    window.harness.state_mut().settings.tmux.repeat_time = 1;
    window.harness.run_steps(2);
    window
        .harness
        .key_press_modifiers(CTRL, egui::Key::ArrowLeft);
    window.harness.run_steps(2);
    window.wait_written("resize-pane");
    std::thread::sleep(std::time::Duration::from_millis(5));
    window
        .harness
        .key_press_modifiers(CTRL, egui::Key::ArrowLeft);
    window.harness.run_steps(3);
    assert!(
        !window.written().contains("resize-pane"),
        "after a pause the arrow goes to the program"
    );
}

#[test]
fn escape_or_a_key_with_no_binding_cancels_the_prefix_and_goes_nowhere() {
    let mut window = two_panes();
    prefix(&mut window);
    assert!(window.shows(&t!("tmux.keys.windows")), "the plate of hints");
    assert!(window.shows(&t!("command.tmux.window.new")));

    window.harness.key_press(egui::Key::Escape);
    window.harness.run_steps(3);
    assert!(!window.shows(&t!("tmux.keys.windows")), "the plate is gone");

    prefix(&mut window);
    typed(&mut window, egui::Key::Y, false, "y");
    window.harness.run_steps(3);
    assert!(!window.shows(&t!("tmux.keys.windows")));
    assert!(
        !window.written().contains("send-keys"),
        "nothing reaches the pane"
    );
}

#[test]
fn the_plate_shows_what_can_act_and_a_key_it_hides_sends_nothing() {
    let mut window = two_panes();
    prefix(&mut window);
    assert!(window.shows(&t!("command.tmux.pane.zoom")), "two panes");
    assert!(!window.shows(&t!("command.tmux.window.next")), "one window");
    assert!(
        !window.shows(&t!("command.tmux.session.next")),
        "one session"
    );
    typed(&mut window, egui::Key::N, false, "n");
    window.harness.run_steps(3);
    assert!(!window.written().contains("select-window"));
}

#[test]
fn a_press_of_the_mouse_cancels_the_prefix() {
    let mut window = two_panes();
    prefix(&mut window);
    window.harness.event(egui::Event::PointerButton {
        pos: egui::pos2(100.0, 100.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    window.harness.run_steps(3);
    assert!(!window.shows(&t!("tmux.keys.windows")));
}

#[test]
fn the_numbers_stand_on_the_panes_and_a_digit_moves_the_focus() {
    let mut window = two_panes();
    prefix(&mut window);
    typed(&mut window, egui::Key::Q, false, "q");
    window.harness.step();
    assert!(window.shows("0") && window.shows("1"), "the numbers");

    typed(&mut window, egui::Key::Num1, false, "1");
    window.step_until("the focus on the other pane", |window| {
        window.shows("\u{229f} 0.1 bash")
    });
    assert!(!window.shows("1"), "the numbers are gone");
    assert!(
        !window.written().contains("send-keys"),
        "the digit goes nowhere"
    );
}
