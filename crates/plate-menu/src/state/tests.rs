use super::*;

fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new("copy", "Copy")
            .detail("ctrl+shift+c")
            .full("Copy the selection"),
        MenuItem::new("paste", "Paste").enabled(false),
        MenuItem::separator(),
        MenuItem::new("profile", "Cat file")
            .enabled(false)
            .children(vec![
                MenuItem::new("profile.zmodem", "zmodem"),
                MenuItem::new("profile.xmodem", "xmodem"),
            ]),
    ]
}

#[test]
fn an_entry_that_carries_entries_is_stepped_into_and_not_chosen() {
    let mut state = MenuState::new(vec![MenuItem::new("profile", "Cat file").children(vec![
        MenuItem::new("profile.zmodem", "zmodem"),
        MenuItem::new("profile.xmodem", "xmodem"),
    ])]);

    assert_eq!(state.choose(0, false), None, "choosing it chooses nothing");
    assert_eq!(state.trail(), vec!["Cat file".to_string()], "it stepped in");
    assert_eq!(state.rows()[1].id, "profile.zmodem");
}

#[test]
fn a_level_opens_on_the_entry_its_parent_names() {
    let mut state = MenuState::new(vec![
        MenuItem::new("parity", "Parity")
            .opens_at("parity.odd")
            .children(vec![
                MenuItem::new("parity.none", "None"),
                MenuItem::new("parity.even", "Even"),
                MenuItem::new("parity.odd", "Odd"),
            ]),
    ]);

    state.step_in();

    let rows = state.rows();
    assert_eq!(rows[state.selected()].id, "parity.odd");
}

#[test]
fn a_level_whose_parent_names_nothing_opens_on_its_first_entry() {
    let mut state = MenuState::new(vec![MenuItem::new("parity", "Parity").children(vec![
        MenuItem::new("parity.none", "None"),
        MenuItem::new("parity.even", "Even"),
    ])]);

    state.step_in();

    let rows = state.rows();
    assert_eq!(rows[state.selected()].id, "parity.none");
}

#[test]
fn the_selection_is_put_on_the_entry_that_is_asked_for() {
    let mut state = MenuState::new(menu());

    assert!(state.select_id("profile"), "the entry is found");
    assert_eq!(state.selected(), 3);
}

#[test]
fn a_sub_entry_is_selected_in_the_level_that_carries_it() {
    let mut state = MenuState::new(vec![
        MenuItem::new("rts", "RTS").children(vec![
            MenuItem::new("rts.down", "Down"),
            MenuItem::new("rts.up", "Up"),
        ]),
        MenuItem::new("dtr", "DTR").children(vec![MenuItem::new("dtr.down", "Down")]),
    ]);

    assert!(state.select_deep("rts.up"));
    assert_eq!(state.trail(), vec!["RTS".to_string()]);
    assert_eq!(state.rows()[state.selected()].id, "rts.up");
    assert!(state.rows()[0].back, "the level above is one step back");
}

#[test]
fn a_sub_entry_that_cannot_be_selected_leaves_the_level_alone() {
    let mut state = MenuState::new(vec![
        MenuItem::new("copy", "Copy"),
        MenuItem::new("rts", "RTS")
            .children(vec![MenuItem::new("rts.down", "Down").enabled(false)]),
    ]);

    assert!(!state.select_deep("rts.down"));
    assert!(state.trail().is_empty());
    assert_eq!(state.rows()[state.selected()].id, "copy");
}

#[test]
fn an_entry_that_is_not_there_leaves_the_selection_alone() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);
    let before = state.selected();

    assert!(!state.select_id("nothing_of_the_sort"));
    assert_eq!(state.selected(), before);
}

#[test]
fn an_entry_that_cannot_be_chosen_is_not_selected() {
    let mut state = MenuState::new(menu());

    assert!(!state.select_id("paste"), "a disabled entry is no target");
    assert_eq!(state.selected(), 0);
}

#[test]
fn the_selection_skips_what_cannot_be_chosen_and_wraps() {
    let mut state = MenuState::new(menu());
    assert_eq!(state.selected(), 0);

    state.move_selection(1);
    assert_eq!(
        state.selected(),
        3,
        "the disabled entry and the line are left out"
    );

    state.move_selection(1);
    assert_eq!(state.selected(), 0, "the end wraps to the start");

    state.move_selection(-1);
    assert_eq!(state.selected(), 3, "and the start to the end");
}

#[test]
fn walking_in_and_out_follows_the_tree() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);

    assert!(state.step_in());
    assert_eq!(state.trail(), vec!["Cat file".to_string()]);
    let rows = state.rows();
    assert_eq!(rows.len(), 3, "the two entries and the row leading back");
    assert!(rows[0].back);
    assert_eq!(rows[0].label, "\u{2039} Cat file");
    assert_eq!(
        state.selected(),
        1,
        "the selection starts on the first entry, not on the way back"
    );
    assert_eq!(state.accept(), Some("profile.zmodem".to_string()));

    assert!(state.step_out());
    assert!(state.trail().is_empty());
    assert!(!state.step_out(), "the top level has nothing to leave");
}

#[test]
fn an_icon_stands_apart_from_the_label_and_on_the_row_back() {
    let mut state = MenuState::new(vec![
        MenuItem::new("w", "0: zsh")
            .icon("\u{1f5d6}")
            .children(vec![MenuItem::new("p", "0.0 zsh")]),
    ]);
    let rows = state.rows();
    assert_eq!(rows[0].label, "0: zsh", "not part of the label");
    assert_eq!(rows[0].icon, "\u{1f5d6}");
    assert!(state.step_in());
    let rows = state.rows();
    assert_eq!(rows[0].label, "\u{2039} 0: zsh");
    assert_eq!(rows[0].icon, "\u{1f5d6}", "the row back carries it");
}

#[test]
fn a_step_out_lands_on_the_entry_it_was_stepped_into_from() {
    let level = |id: &str, children| MenuItem::new(id, id).children(children);
    let mut state = MenuState::new(vec![
        MenuItem::new("a", "a"),
        MenuItem::new("b", "b"),
        level(
            "c",
            vec![
                MenuItem::new("c.0", "c.0"),
                level("c.1", vec![MenuItem::new("c.1.0", "c.1.0")]),
            ],
        ),
        MenuItem::new("d", "d"),
    ]);
    state.move_selection(2);
    assert!(state.step_in());
    state.move_selection(1);
    assert!(state.step_in());

    assert!(state.step_out());
    assert_eq!(state.rows()[state.selected()].id, "c.1");
    assert!(state.step_out());
    assert_eq!(
        state.rows()[state.selected()].id,
        "c",
        "not the first entry"
    );

    state.step_in();
    state.move_selection(-1);
    assert!(state.rows()[state.selected()].back);
    assert_eq!(state.accept(), None);
    assert_eq!(
        state.rows()[state.selected()].id,
        "c",
        "the row back as well"
    );
}

#[test]
fn the_row_that_names_the_level_above_leads_back_to_it() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);
    state.step_in();

    state.move_selection(-1);
    assert!(state.rows()[state.selected()].back);
    assert_eq!(state.accept(), None);
    assert!(state.trail().is_empty(), "choosing it steps back out");
}

#[test]
fn a_parent_that_cannot_be_chosen_is_walked_into_by_enter() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);

    assert_eq!(state.accept(), None);
    assert_eq!(state.trail(), vec!["Cat file".to_string()]);
}

#[test]
fn typing_searches_the_whole_tree_and_leaving_the_query_restores_the_level() {
    let mut state = MenuState::new(menu());
    state.push_query("xmo");

    let rows = state.rows();
    assert_eq!(
        rows.first().map(|row| row.id.clone()),
        Some("profile.xmodem".to_string())
    );
    assert_eq!(rows[0].parents, "Cat file");
    assert_eq!(state.accept(), Some("profile.xmodem".to_string()));

    assert!(state.step_out());
    assert_eq!(state.query(), "");
    assert_eq!(state.rows().len(), 4);
}

#[test]
fn an_entry_that_is_choosable_is_chosen_and_still_stepped_into() {
    let mut state = MenuState::new(vec![
        MenuItem::new("source:/dev/ttyUSB0", "/dev/ttyUSB0")
            .choosable(true)
            .children(vec![MenuItem::new(
                "source.default:/dev/ttyUSB0",
                "At start",
            )]),
    ]);

    assert_eq!(
        state.accept(),
        Some("source:/dev/ttyUSB0".to_string()),
        "Enter chooses it instead of walking into it"
    );

    assert!(state.step_in(), "the arrow still walks into it");
    assert_eq!(state.rows()[1].id, "source.default:/dev/ttyUSB0");
}

#[test]
fn entries_put_in_place_of_the_others_keep_the_selection_and_the_query() {
    let mut state = MenuState::new(menu());
    state.push_query("xmodem");
    assert!(state.select_id("profile.xmodem"), "the query finds it");

    state.refill(vec![
        MenuItem::new("added", "Added"),
        MenuItem::new("profile", "Cat file").children(vec![
            MenuItem::new("profile.zmodem", "zmodem"),
            MenuItem::new("profile.xmodem", "xmodem"),
        ]),
    ]);

    assert_eq!(state.query(), "xmodem", "the query is left alone");
    assert_eq!(state.rows()[state.selected()].id, "profile.xmodem");
}

#[test]
fn an_entry_that_is_gone_leaves_the_selection_on_the_first_one() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);
    assert_eq!(state.rows()[state.selected()].id, "profile");

    state.refill(vec![MenuItem::new("copy", "Copy")]);

    assert_eq!(state.rows()[state.selected()].id, "copy");
}

#[test]
fn a_level_that_is_gone_is_left_for_the_one_above_it() {
    let mut state = MenuState::new(menu());
    state.move_selection(1);
    state.step_in();

    state.refill(vec![MenuItem::new("copy", "Copy")]);

    assert!(state.trail().is_empty());
    assert_eq!(state.rows()[state.selected()].id, "copy");
}

/// Entries that are found by `usb` and by nothing past it.
fn ports() -> Vec<MenuItem> {
    vec![
        MenuItem::new("usb0", "ttyUSB0"),
        MenuItem::new("usb1", "ttyUSB1"),
        MenuItem::new("acm0", "ttyACM0"),
    ]
}

/// `Backspace` over a query nothing answers takes back everything past the
/// last beginning that something answers, in one press.
#[test]
fn a_query_that_finds_nothing_is_cut_back_to_what_still_finds_something() {
    let mut state = MenuState::new(ports());
    state.push_query("usbxyz");
    assert!(state.nothing_found());

    assert!(state.pop_query());
    assert_eq!(state.query(), "usb");
    assert_eq!(state.caret(), 3);
    assert!(!state.nothing_found());

    assert!(state.pop_query());
    assert_eq!(state.query(), "us", "a query with hits loses one character");
}

/// A word typed in the wrong keyboard layout finds nothing from its first
/// letter on, and one press clears it.
#[test]
fn a_query_that_found_nothing_from_its_first_character_is_cleared() {
    let mut state = MenuState::new(ports());
    state.push_query("гыи");

    assert!(state.pop_query());
    assert_eq!(state.query(), "");
    assert_eq!(state.rows().len(), 3, "the level is shown again");
}

/// The caret is where typed text goes and what `Backspace` deletes before.
#[test]
fn text_goes_in_at_the_caret() {
    let mut state = MenuState::new(ports());
    state.push_query("tyb");
    state.caret_home();
    state.push_query("t");
    assert_eq!((state.query(), state.caret()), ("ttyb", 1));
    state.pop_query();
    assert_eq!((state.query(), state.caret()), ("tyb", 0));
    state.caret_end();
    assert_eq!(state.caret(), 3);

    let mut state = MenuState::new(ports());
    state.push_query("usb");
    state.caret_home();
    state.push_query("x");
    assert!(state.nothing_found());
    state.pop_query();
    assert_eq!(
        state.query(),
        "",
        "with nothing found, the text right of the caret goes as well"
    );
}

#[test]
fn a_query_is_cleared_and_the_level_comes_back() {
    let mut state = MenuState::new(ports());
    assert!(!state.clear_query(), "there was none");
    state.push_query("acm");
    assert!(state.clear_query());
    assert_eq!((state.query(), state.caret()), ("", 0));
    assert_eq!(state.rows().len(), 3);
}

/// Ten entries with a line and a disabled entry among them.
fn long() -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = (0..10)
        .map(|index| MenuItem::new(format!("id{index}"), format!("entry {index}")))
        .collect();
    items[4] = MenuItem::separator();
    items[9] = MenuItem::new("id9", "entry 9").enabled(false);
    items
}

/// A page is walked without wrapping, and lands on an entry: never on a
/// line, never past the last entry that can be chosen.
#[test]
fn a_page_stops_at_the_ends_and_lands_on_an_entry() {
    let mut state = MenuState::new(long());

    state.move_page(4);
    assert_eq!(state.selected(), 3, "the line at 4 is passed back over");
    state.move_page(4);
    assert_eq!(state.selected(), 7);
    state.move_page(4);
    assert_eq!(state.selected(), 8, "the last entry that can be chosen");
    state.move_page(4);
    assert_eq!(state.selected(), 8, "and no further");

    state.move_page(-6);
    assert_eq!(state.selected(), 2);
    state.move_page(-6);
    assert_eq!(state.selected(), 0, "and no wrapping at the start");
}

#[test]
fn the_last_entry_is_one_that_can_be_chosen() {
    let mut state = MenuState::new(long());
    state.select_last();
    assert_eq!(state.selected(), 8);
    state.select_first();
    assert_eq!(state.selected(), 0);
}

/// A window moved away from the selection takes it along, to the row shown
/// nearest to where it was.
#[test]
fn a_selection_the_window_left_lands_on_the_nearest_row_shown() {
    let mut state = MenuState::new(long());

    state.select_shown(3, 6);
    assert_eq!(state.selected(), 3, "the window moved down past it");

    state.select_shown(0, 3);
    assert_eq!(state.selected(), 2, "the window moved up past it");

    state.select_shown(1, 4);
    assert_eq!(state.selected(), 2, "a selection shown stays");
}

/// The zone of an entry that is chosen steps into it, the way `Right` does.
#[test]
fn an_entry_that_is_chosen_is_stepped_into_by_its_zone() {
    let mut state = MenuState::new(vec![
        MenuItem::new("copy", "Copy"),
        MenuItem::new("source", "/dev/ttyUSB0")
            .choosable(true)
            .children(vec![MenuItem::new("default", "At start")]),
    ]);
    assert!(state.rows()[1].is_split());
    assert!(!state.rows()[0].is_split());

    assert!(state.step_into(1));
    assert_eq!(state.trail(), vec!["/dev/ttyUSB0".to_string()]);
    assert_eq!(state.rows()[state.selected()].id, "default");
    assert!(
        !state.rows()[0].is_split(),
        "the row leading back is never split"
    );
}

#[test]
fn the_query_is_shortened_one_character_at_a_time() {
    let mut state = MenuState::new(menu());
    state.push_query("co");
    assert!(state.pop_query());
    assert_eq!(state.query(), "c");
    assert!(state.pop_query());
    assert!(!state.pop_query());
}
