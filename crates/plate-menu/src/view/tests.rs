use super::*;

/// A context with a window of this height, after one frame, so the fonts
/// and the style are there.
fn context(height: f32) -> Context {
    let context = Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(800.0, height),
        )),
        ..Default::default()
    };
    let mut output = context.run_ui(input, |_| {});
    output.textures_delta.clear();
    context
}

fn items(count: usize) -> Vec<MenuItem> {
    (0..count)
        .map(|index| MenuItem::new(format!("id{index}"), format!("entry {index}")))
        .collect()
}

/// A menu says whether it holds an entry, at any level, and a closed one
/// holds none.
#[test]
fn a_menu_says_what_it_holds() {
    let mut menu = PlateMenu::new();
    assert!(!menu.holds("id0"));
    let mut entries = items(2);
    entries.push(
        MenuItem::new("parent", "parent").children(
            items(1)
                .into_iter()
                .map(|item| MenuItem::new("deep", item.label))
                .collect(),
        ),
    );
    menu.open(entries).expect("an entry can be chosen");
    assert!(menu.holds("id1") && menu.holds("deep"));
    assert!(!menu.holds("absent"));
    menu.close();
    assert!(!menu.holds("id1"));
}

#[test]
fn every_glyph_the_menu_draws_is_in_the_fonts_of_the_toolkit() {
    let context = context(600.0);
    let missing: Vec<&str> = GLYPHS
        .iter()
        .copied()
        .filter(|glyph| {
            !context.fonts_mut(|fonts| fonts.has_glyphs(&FontId::proportional(14.0), glyph))
        })
        .collect();

    assert!(
        missing.is_empty(),
        "no font of the toolkit carries these: {missing:?}"
    );
}

/// One frame of the menu with `Enter` pressed, and the modifiers held while
/// it was — as the platform reports them, which is a change of the
/// modifiers and then the key.
fn enter(context: &Context, menu: &mut PlateMenu, held: egui::Modifiers) -> Option<Chosen> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(800.0, 600.0),
        )),
        events: vec![
            egui::Event::ModifiersChanged(held),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: held,
            },
        ],
        ..Default::default()
    };

    let mut chosen = None;
    let mut output = context.run_ui(input, |ui| chosen = menu.show(ui.ctx()));
    output.textures_delta.clear();
    chosen
}

/// The entry that was chosen comes back with the keys that were held while
/// it was, because both ways of choosing it choose the same entry and what
/// the second of them means is the caller's to say.
#[test]
fn what_was_held_while_an_entry_was_chosen_comes_back_with_it() {
    let context = context(600.0);

    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");
    let plain = enter(&context, &mut menu, egui::Modifiers::NONE).expect("an entry is chosen");
    assert_eq!(plain.id, "id0");
    assert!(!plain.held.shift);

    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");
    let shifted = enter(&context, &mut menu, egui::Modifiers::SHIFT).expect("an entry is chosen");
    assert_eq!(shifted.id, "id0", "the same entry, chosen the other way");
    assert!(shifted.held.shift);
}

#[test]
fn a_menu_without_a_reachable_entry_is_refused() {
    let mut menu = PlateMenu::new();
    let refused = menu.open(vec![
        MenuItem::separator(),
        MenuItem::new("x", "X").enabled(false),
    ]);

    assert!(matches!(refused, Err(MenuError::Empty)));
    assert!(!menu.is_open());
}

/// A menu that does not fit gives the room of both strips from what fits;
/// one that fits keeps every plate and has no strip.
#[test]
fn the_default_plate_count_fits_the_window_less_a_fifth() {
    let context = context(600.0);
    let menu = PlateMenu::new();
    let height = plate_height_of(&context);
    let room = 600.0 * HEIGHT_SHARE;
    let fits = (room / height).floor() as usize;
    let expected = ((room - 2.0 * strip_step(&context)) / height).floor() as usize;

    assert!(expected > 1, "a window of 600 points holds several plates");
    assert_eq!(plate_count(&context, menu.max_plates, 100), expected);
    assert_eq!(
        plate_count(&context, menu.max_plates, fits),
        fits,
        "a menu that fits needs no strip"
    );
    assert_eq!(
        plate_count(&context, menu.max_plates, 3),
        3,
        "a short menu is not padded"
    );
}

#[test]
fn a_given_plate_count_is_kept() {
    let context = context(2000.0);
    let menu = PlateMenu::new().max_plates(4);

    assert_eq!(plate_count(&context, menu.max_plates, 100), 4);
    assert_eq!(plate_count(&context, menu.max_plates, 2), 2);
}

#[test]
fn the_window_walks_with_the_selection_one_row_at_a_time() {
    let mut menu = PlateMenu::new();

    assert_eq!(menu.window(0, 3, 10), 0);
    assert_eq!(
        menu.window(2, 3, 10),
        0,
        "what is visible stays where it is"
    );
    assert_eq!(menu.window(3, 3, 10), 1, "one row down, one row along");
    assert_eq!(menu.window(9, 3, 10), 7, "the end shows the last plates");
    assert_eq!(menu.window(1, 3, 10), 1, "and back up the same way");
    assert_eq!(menu.window(0, 3, 10), 0);
    assert_eq!(menu.window(0, 20, 10), 0, "a menu that fits never moves");
}

#[test]
fn the_wheel_walks_the_plates_and_is_taken_out_of_the_input() {
    let context = context(600.0);
    let mut menu = PlateMenu::new();
    menu.open(items(5)).expect("the menu opens");

    let scrolled = wheel(&context, &mut menu, -SCROLL_STEP * 3.0);
    assert!(
        menu.selected().is_some_and(|selected| selected > 0),
        "turning the wheel down walks towards the end"
    );
    assert_eq!(
        scrolled,
        Vec2::ZERO,
        "and what it read is gone from the input"
    );

    let down = menu.selected().expect("the menu is open");
    wheel(&context, &mut menu, SCROLL_STEP * 3.0);
    assert!(
        menu.selected().is_some_and(|selected| selected < down),
        "turning it up walks back"
    );
}

/// Turns the wheel for as many frames as the toolkit needs to hand the
/// whole delta over, and reports what was left in the input.
fn wheel(context: &Context, menu: &mut PlateMenu, delta: f32) -> Vec2 {
    let mut left = Vec2::ZERO;
    for frame in 0..8 {
        let events = if frame == 0 {
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }]
        } else {
            Vec::new()
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            menu.show(ui.ctx());
            left = ui.input(|input| input.smooth_scroll_delta);
        });
        output.textures_delta.clear();
    }
    left
}

#[test]
fn opening_on_an_entry_puts_the_selection_there() {
    let mut menu = PlateMenu::new();
    menu.open_at(items(5), "id2").expect("the menu opens");

    assert_eq!(menu.selected(), Some(2));
}

#[test]
fn opening_on_an_entry_that_is_not_there_starts_at_the_first() {
    let mut menu = PlateMenu::new();
    menu.open_at(items(5), "nothing_of_the_sort")
        .expect("the menu opens");

    assert_eq!(menu.selected(), Some(0));
}

#[test]
fn the_keys_the_menu_walks_by_are_taken_out_of_the_input() {
    let context = Context::default();
    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");

    let mut left = true;
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(800.0, 600.0),
        )),
        events: vec![egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| {
        menu.show(ui.ctx());
        left = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));
    });
    output.textures_delta.clear();

    assert_eq!(menu.selected(), Some(1), "the menu walked");
    assert!(!left, "and nothing behind it can walk along");
}

#[test]
fn the_layer_is_the_same_open_or_closed() {
    let mut menu = PlateMenu::new().with_id("a menu of its own");
    let closed = menu.layer_id();

    menu.open(items(2)).expect("the menu opens");

    assert_eq!(menu.layer_id(), closed);
    assert_eq!(menu.layer_id().order, egui::Order::Foreground);
}

#[test]
fn a_click_beside_a_menu_that_ignores_it_leaves_it_open() {
    let context = Context::default();
    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");
    menu.beside(Beside::Ignored);

    click(&context, &mut menu, egui::Pos2::new(5.0, 5.0));
    assert!(menu.is_open(), "a click beside it is no answer");

    menu.beside(Beside::Answers);
    click(&context, &mut menu, egui::Pos2::new(5.0, 5.0));
    assert!(!menu.is_open(), "and it closes again when it may");
}

/// Draws the menu for as many frames as a click needs and clicks there.
fn click(context: &Context, menu: &mut PlateMenu, at: egui::Pos2) {
    for frame in 0..3 {
        let events = if frame == 2 {
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        } else {
            Vec::new()
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            menu.show(ui.ctx());
        });
        output.textures_delta.clear();
    }
}

#[test]
fn a_notice_belongs_to_the_menu_it_was_set_on() {
    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");
    menu.notice("what happened");
    assert_eq!(menu.notice.as_deref(), Some("what happened"));

    menu.notice("   ");
    assert_eq!(menu.notice, None, "a notice of blanks is no notice");

    menu.notice("what happened");
    menu.open(items(3)).expect("the menu opens");
    assert_eq!(menu.notice, None, "an opening clears it");
}

/// One frame of a window 800 by 600 with these events.
fn frame(context: &Context, menu: &mut PlateMenu, events: Vec<egui::Event>) -> Option<Chosen> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut chosen = None;
    let mut output = context.run_ui(input, |ui| chosen = menu.show(ui.ctx()));
    output.textures_delta.clear();
    chosen
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn press(context: &Context, menu: &mut PlateMenu, pressed: egui::Key) {
    frame(context, menu, vec![key(pressed, egui::Modifiers::NONE)]);
}

fn query(menu: &PlateMenu) -> String {
    menu.state
        .as_ref()
        .map(|state| state.query().to_string())
        .unwrap_or_default()
}

/// The first `Esc` takes the query away, the second the menu.
#[test]
fn escape_clears_the_query_before_it_closes_the_menu() {
    let context = context(600.0);
    let mut menu = PlateMenu::new();
    menu.open(items(5)).expect("the menu opens");
    frame(&context, &mut menu, vec![egui::Event::Text("3".into())]);
    assert_eq!(query(&menu), "3");

    press(&context, &mut menu, egui::Key::Escape);
    assert!(menu.is_open(), "the query went, the menu stays");
    assert_eq!(query(&menu), "");

    press(&context, &mut menu, egui::Key::Escape);
    assert!(!menu.is_open());
}

/// `Home` and `End` walk the list while nothing is typed and move the caret
/// while something is; the pages walk the list either way.
#[test]
fn home_and_end_walk_the_list_or_the_query() {
    let context = context(600.0);
    let mut menu = PlateMenu::new().max_plates(4);
    menu.open(items(20)).expect("the menu opens");
    frame(&context, &mut menu, Vec::new());

    press(&context, &mut menu, egui::Key::End);
    assert_eq!(menu.selected(), Some(19));
    press(&context, &mut menu, egui::Key::Home);
    assert_eq!(menu.selected(), Some(0));
    press(&context, &mut menu, egui::Key::PageDown);
    assert_eq!(menu.selected(), Some(4), "a page is the plates shown");
    press(&context, &mut menu, egui::Key::PageUp);
    assert_eq!(menu.selected(), Some(0));

    frame(&context, &mut menu, vec![egui::Event::Text("entry".into())]);
    press(&context, &mut menu, egui::Key::Home);
    let caret = menu.state.as_ref().map(MenuState::caret);
    assert_eq!(caret, Some(0), "the caret went to the start");
    press(&context, &mut menu, egui::Key::End);
    let caret = menu.state.as_ref().map(MenuState::caret);
    assert_eq!(caret, Some(5), "and to the end");
}

/// A paste goes into the query on one line, `Ctrl+Backspace` clears it,
/// and neither the paste nor `Tab` is left to what is behind the menu.
#[test]
fn a_paste_and_tab_belong_to_the_menu() {
    let context = context(600.0);
    let mut menu = PlateMenu::new();
    menu.open(items(5)).expect("the menu opens");

    let mut left = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(800.0, 600.0),
        )),
        events: vec![
            egui::Event::Paste("entry\n1".into()),
            key(egui::Key::Tab, egui::Modifiers::NONE),
        ],
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| {
        menu.show(ui.ctx());
        left = ui.input(|input| input.events.clone());
    });
    output.textures_delta.clear();

    assert_eq!(query(&menu), "entry 1");
    assert!(left.is_empty(), "nothing is left behind: {left:?}");

    frame(
        &context,
        &mut menu,
        vec![key(egui::Key::Backspace, egui::Modifiers::COMMAND)],
    );
    assert_eq!(query(&menu), "");
    assert!(menu.is_open());
}

/// The rows a window leaves out on each side, lines not counted.
#[test]
fn the_strips_count_the_entries_left_out() {
    let mut entries = items(6);
    entries.insert(1, MenuItem::separator());
    entries.insert(6, MenuItem::separator());
    let state = MenuState::new(entries);
    let rows = state.rows();

    assert_eq!(hidden(&rows, 0, 3), (0, 4), "the line at 6 is not counted");
    assert_eq!(hidden(&rows, 3, 5), (2, 2), "nor the one at 1");
    assert_eq!(hidden(&rows, 5, 8), (4, 0));
}

/// The window keeps its size while it walks from the first row to the
/// last, whichever strips are drawn.
#[test]
fn the_menu_keeps_its_height_while_the_window_walks() {
    let context = context(600.0);
    let mut menu = PlateMenu::new();
    menu.open(items(60)).expect("the menu opens");
    frame(&context, &mut menu, Vec::new());
    let at_first = (menu.shown, menu.rect.height());

    press(&context, &mut menu, egui::Key::PageDown);
    frame(&context, &mut menu, Vec::new());
    let in_the_middle = (menu.shown, menu.rect.height());

    press(&context, &mut menu, egui::Key::End);
    frame(&context, &mut menu, Vec::new());
    let at_last = (menu.shown, menu.rect.height());

    assert!(at_first.0 < 60);
    assert_eq!(at_first, in_the_middle);
    assert_eq!(at_first, at_last);
}

/// While a query is typed the menu keeps the top edge it had, and gets
/// shorter from the bottom.
#[test]
fn a_query_keeps_the_top_of_the_menu() {
    let context = context(600.0);
    let mut menu = PlateMenu::new();
    menu.open(items(12)).expect("the menu opens");
    frame(&context, &mut menu, Vec::new());
    frame(&context, &mut menu, Vec::new());
    let before = menu.rect;

    frame(&context, &mut menu, vec![egui::Event::Text("1".into())]);
    frame(&context, &mut menu, Vec::new());
    let typed = menu.rect;
    frame(&context, &mut menu, vec![egui::Event::Text("1".into())]);
    frame(&context, &mut menu, Vec::new());
    let narrowed = menu.rect;

    assert!(
        typed.height() < before.height(),
        "{typed:?} under {before:?}"
    );
    assert!(narrowed.height() < typed.height());
    assert!(
        (typed.top() - before.top()).abs() < 0.5,
        "{typed:?} under {before:?}"
    );
    assert!((narrowed.top() - before.top()).abs() < 0.5);
    assert!(narrowed.width() >= before.width() - 0.5, "no narrower");

    press(&context, &mut menu, egui::Key::Escape);
    frame(&context, &mut menu, Vec::new());
    assert_eq!(menu.rect, before, "and the menu stands where it stood");
}

/// Pointer events that press at `from`, move by `by` in `steps` and
/// release, a frame each.
fn drag(
    context: &Context,
    menu: &mut PlateMenu,
    from: Pos2,
    by: Vec2,
    steps: usize,
) -> Option<Chosen> {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut chosen = frame(
        context,
        menu,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    for step in 1..=steps {
        let at = from + by * (step as f32 / steps as f32);
        chosen = chosen.or(frame(context, menu, vec![egui::Event::PointerMoved(at)]));
    }
    chosen.or(frame(context, menu, vec![button(from + by, false)]))
}

/// A drag moves the window a row for every plate the pointer travelled,
/// takes the selection along, stops at the end and chooses nothing.
#[test]
fn a_drag_moves_the_window_and_chooses_nothing() {
    let context = context(600.0);
    let mut menu = PlateMenu::new().max_plates(5);
    menu.open(items(20)).expect("the menu opens");
    frame(&context, &mut menu, Vec::new());
    frame(&context, &mut menu, Vec::new());
    let step = plate_step(&context);
    let on_the_plates = menu.rect.center();

    let chosen = drag(
        &context,
        &mut menu,
        on_the_plates,
        Vec2::new(0.0, -3.2 * step),
        4,
    );
    assert_eq!(chosen, None, "the release chooses nothing");
    assert!(menu.is_open());
    assert_eq!(menu.first, 3, "up by three plates, three rows on");
    assert_eq!(menu.selected(), Some(3), "the selection came along");

    drag(
        &context,
        &mut menu,
        on_the_plates,
        Vec2::new(0.0, -40.0 * step),
        8,
    );
    assert_eq!(menu.first, 15, "the window stops at the last rows");
    assert_eq!(menu.selected(), Some(15));

    drag(
        &context,
        &mut menu,
        on_the_plates,
        Vec2::new(0.0, 2.0 * step),
        4,
    );
    assert_eq!(menu.first, 13, "down shows the rows above");
    assert_eq!(menu.selected(), Some(15), "a selection still shown stays");
}

/// A drag that starts beside the menu is not the menu's: nothing moves and
/// nothing closes.
#[test]
fn a_drag_beside_the_menu_moves_nothing() {
    let context = context(600.0);
    let mut menu = PlateMenu::new().max_plates(5);
    menu.open(items(20)).expect("the menu opens");
    frame(&context, &mut menu, Vec::new());

    drag(
        &context,
        &mut menu,
        Pos2::new(5.0, 300.0),
        Vec2::new(0.0, -200.0),
        4,
    );
    assert!(menu.is_open());
    assert_eq!(menu.first, 0);
    assert_eq!(menu.selected(), Some(0));
}

#[test]
fn opening_puts_the_selection_on_the_first_entry() {
    let mut menu = PlateMenu::new();
    menu.open(items(3)).expect("the menu opens");

    assert!(menu.is_open());
    menu.close();
    assert!(!menu.is_open());
}
