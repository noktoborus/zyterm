//! Emulation behaviour driven by plain byte input.

use zyt_term::{
    Color, SearchDirection, SearchKind, SearchOptions, SelectionKind, SelectionStep, Terminal,
    TerminalConfig, TerminalEvent, TerminalModes,
};

fn terminal() -> Terminal {
    Terminal::new(TerminalConfig {
        columns: 20,
        rows: 4,
        scrollback: 100,
        clipboard: zyt_term::ClipboardAccess::CopyPaste,
    })
    .expect("terminal is created")
}

fn line_text(content: &zyt_term::RenderableContent, row: usize) -> String {
    (0..content.columns)
        .map(|column| content.cell(column, row).map(|cell| cell.ch).unwrap_or(' '))
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn plain_text_lands_in_the_grid() {
    let mut term = terminal();
    term.feed(b"hello\r\nworld");
    let content = term.content();
    assert_eq!(line_text(&content, 0), "hello");
    assert_eq!(line_text(&content, 1), "world");
    assert_eq!(content.cursor.map(|cursor| cursor.column), Some(5));
}

#[test]
fn colors_and_attributes_are_reported() {
    let mut term = terminal();
    term.feed(b"\x1b[1;31mred\x1b[0m");
    let content = term.content();
    let cell = content.cell(0, 0).expect("cell exists");
    assert_eq!(cell.fg, Color::Palette(1));
    assert!(cell.style.bold);
}

#[test]
fn queries_are_answered_through_the_output_buffer() {
    let mut term = terminal();
    term.feed(b"\x1b[6n");
    let answer = term.take_output();
    assert!(!answer.is_empty());
    assert!(answer.starts_with(b"\x1b["));
}

#[test]
fn title_and_bell_are_reported_as_events() {
    let mut term = terminal();
    term.feed(b"\x1b]0;device\x07\x07");
    let events = term.take_events();
    assert!(events.contains(&TerminalEvent::Title("device".to_string())));
    assert!(events.contains(&TerminalEvent::Bell));
}

#[test]
fn a_view_that_was_scrolled_back_stays_where_it_is() {
    let mut term = terminal();
    for line in 0..40 {
        term.feed(format!("line{line}\r\n").as_bytes());
    }

    term.scroll(5);
    let before = line_text(&term.content(), 0);
    assert_eq!(term.content().display_offset, 5);

    for line in 40..50 {
        term.feed(format!("line{line}\r\n").as_bytes());
    }

    assert_eq!(
        line_text(&term.content(), 0),
        before,
        "what is being read does not move under the reader"
    );
    assert_eq!(
        term.content().display_offset,
        15,
        "the view is that many lines further from the end now"
    );

    term.scroll_to_bottom();
    assert_eq!(
        term.content().display_offset,
        0,
        "and the end is one step away"
    );
}

#[test]
fn scrollback_is_reachable_and_selection_returns_text() {
    let mut term = terminal();
    for line in 0..10 {
        term.feed(format!("line{line}\r\n").as_bytes());
    }
    assert!(term.content().history_size > 0);

    term.scroll(3);
    assert_eq!(term.content().display_offset, 3);
    term.scroll_to_bottom();
    assert_eq!(term.content().display_offset, 0);

    term.selection_start(SelectionKind::Lines, 0, 0)
        .expect("position is inside the grid");
    term.selection_update(4, 0)
        .expect("position is inside the grid");
    let selected = term.selected_text().expect("selection has text");
    assert!(selected.contains("line"));
}

#[test]
fn modes_follow_the_escape_sequences() {
    let mut term = terminal();
    assert_eq!(term.modes(), TerminalModes::default());
    term.feed(b"\x1b[?1h\x1b[?2004h");
    let modes = term.modes();
    assert!(modes.app_cursor);
    assert!(modes.bracketed_paste);
}

#[test]
fn resize_keeps_the_terminal_usable() {
    let mut term = terminal();
    term.feed(b"abc");
    term.resize(40, 10).expect("size is valid");
    assert_eq!(term.size(), (40, 10));
    assert!(term.resize(1, 0).is_err());
    let content = term.content();
    assert_eq!(content.columns, 40);
    assert_eq!(content.rows, 10);
}

#[test]
fn snapshots_are_only_needed_after_a_change() {
    let mut term = terminal();
    assert!(term.is_dirty());

    let mut content = zyt_term::RenderableContent::default();
    term.render_into(&mut content);
    assert!(!term.is_dirty());

    term.feed(b"");
    assert!(!term.is_dirty());

    term.feed(b"x");
    assert!(term.is_dirty());
    term.render_into(&mut content);

    term.scroll(0);
    assert!(!term.is_dirty());
    term.selection_clear();
    assert!(!term.is_dirty());

    term.resize(40, 12).expect("size is valid");
    assert!(term.is_dirty());
}

#[test]
fn hyperlinks_land_in_the_snapshot() {
    let mut term = terminal();
    term.feed(b"\x1b]8;;https://example.org\x07link\x1b]8;;\x07 plain");

    let content = term.content();
    assert_eq!(content.link_at(0, 0), Some("https://example.org"));
    assert_eq!(content.link_at(3, 0), Some("https://example.org"));
    assert_eq!(content.link_at(5, 0), None);
    assert_eq!(content.links.len(), 1);
}

#[test]
fn the_working_directory_and_a_notification_are_events() {
    let mut term = terminal();
    term.feed(b"\x1b]7;file://host/tmp\x07\x1b]777;notify;Build;done\x07\x1b]133;A\x07");

    let events = term.take_events();
    assert!(events.contains(&TerminalEvent::WorkingDirectory("/tmp".to_string())));
    assert!(events.contains(&TerminalEvent::Notification {
        kind: zyt_term::NotificationKind::Titled,
        title: "Build".to_string(),
        body: "done".to_string(),
    }));
    assert!(events.contains(&TerminalEvent::Mark(zyt_term::MarkKind::PromptStart)));
}

/// The command a shell marked is the text it echoed between the two marks, and
/// the marks arrive in the same chunk as that text, so nothing but a parser
/// walking the chunk in step with the sniffer can find it.
#[test]
fn a_marked_command_is_read_back_out_of_the_grid() {
    let mut term = terminal();
    term.feed(b"\x1b]133;A\x07user@host:~$ \x1b]133;B\x07ls -la /tmp\r\n\x1b]133;C\x07");

    assert!(
        term.take_events()
            .contains(&TerminalEvent::Command("ls -la /tmp".to_string()))
    );
}

#[test]
fn a_command_typed_over_several_rows_comes_back_whole() {
    let mut term = terminal();
    term.feed(b"\x1b]133;B\x07echo one \\\r\n  two\r\n\x1b]133;C\x07");

    let events = term.take_events();
    let found = events.iter().find_map(|event| match event {
        TerminalEvent::Command(line) => Some(line.clone()),
        _ => None,
    });
    assert_eq!(found, Some("echo one \\\n  two".to_string()));
}

/// A command is recorded when its output starts, which is before anything is
/// known about how it ended, so what it exits with plays no part: a command
/// that failed is exactly the one somebody wants to fetch back and correct.
#[test]
fn a_command_that_failed_is_kept_like_any_other() {
    let mut term = terminal();
    term.feed(b"\x1b]133;B\x07false\r\n\x1b]133;C\x07\x1b]133;D;1\x07");

    assert!(
        term.take_events()
            .contains(&TerminalEvent::Command("false".to_string()))
    );
}

/// The screen moves while a command is being entered: the newline that enters
/// it pushes every row up by one, and on a screen that is already full that
/// costs the row the command was typed on its number. A place recorded before
/// that has to still name the same text afterwards.
///
/// The bytes are the shape bash emits with the marks turned on, bracketed paste
/// and all: prompt, `133;B`, the echo of the line, the newline, `133;C`.
#[test]
fn a_command_is_found_again_after_the_screen_scrolled_under_it() {
    let mut term = terminal();
    for line in 0..8 {
        term.feed(format!("filling row {line}\r\n").as_bytes());
    }

    term.feed(b"\x1b[?2004hbash$ \x1b]133;B\x07uname -s\r\n\x1b[?2004l\r\x1b]133;C\x07");
    term.feed(b"Linux\r\n\x1b]133;D;0\x07");

    assert!(
        term.take_events()
            .contains(&TerminalEvent::Command("uname -s".to_string()))
    );
}

/// A command whose place has been pushed out of the scrollback altogether is
/// answered with nothing rather than with whatever stands there now.
#[test]
fn a_command_scrolled_out_of_the_scrollback_is_not_guessed_at() {
    let mut term = Terminal::new(TerminalConfig {
        columns: 20,
        rows: 4,
        scrollback: 1,
        clipboard: zyt_term::ClipboardAccess::CopyPaste,
    })
    .expect("terminal is created");

    term.feed(b"$ \x1b]133;B\x07uname -s\r\n");
    for line in 0..40 {
        term.feed(format!("output {line}\r\n").as_bytes());
    }
    term.feed(b"\x1b]133;C\x07");

    assert!(
        !term
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::Command(_)))
    );
}

#[test]
fn a_prompt_nobody_typed_at_reports_no_command() {
    let mut term = terminal();
    term.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07\x1b]133;C\x07");

    assert!(
        !term
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::Command(_)))
    );
}

#[test]
fn reading_the_clipboard_follows_the_setting() {
    let mut term = terminal();
    term.set_clipboard_access(zyt_term::ClipboardAccess::CopyOnly);
    term.feed(b"\x1b]52;c;?\x07");
    assert!(
        !term
            .take_events()
            .contains(&TerminalEvent::ClipboardRequest)
    );
    assert!(!term.clipboard_requested());

    term.feed(b"\x1b]52;c;aGVsbG8=\x07");
    assert!(
        term.take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::ClipboardStore(_)))
    );

    term.set_clipboard_access(zyt_term::ClipboardAccess::Disabled);
    term.feed(b"\x1b]52;c;aGVsbG8=\x07");
    assert!(
        !term
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::ClipboardStore(_)))
    );
}

#[test]
fn a_clipboard_request_waits_for_an_answer() {
    let mut term = terminal();
    term.feed(b"\x1b]52;c;?\x07");

    assert!(
        term.take_events()
            .contains(&TerminalEvent::ClipboardRequest)
    );
    assert!(term.clipboard_requested());
    assert!(term.take_output().is_empty());

    term.answer_clipboard(Some("hello"));
    let answer = term.take_output();
    assert!(!answer.is_empty());
    assert!(String::from_utf8_lossy(&answer).contains("52;"));
    assert!(!term.clipboard_requested());

    term.feed(b"\x1b]52;c;?\x07");
    let _ = term.take_events();
    term.answer_clipboard(None);
    assert!(term.take_output().is_empty());
    assert!(!term.clipboard_requested());
}

#[test]
fn the_colors_a_program_sets_reach_the_snapshot() {
    let mut term = terminal();
    assert!(term.content().background.is_none());

    term.feed(b"\x1b]11;#202040\x07\x1b]10;#e0e0f0\x07");
    let content = term.content();
    assert_eq!(
        content.background,
        Some(zyt_term::Rgb {
            r: 0x20,
            g: 0x20,
            b: 0x40
        })
    );
    assert_eq!(
        content.foreground,
        Some(zyt_term::Rgb {
            r: 0xe0,
            g: 0xe0,
            b: 0xf0
        })
    );

    term.feed(b"\x1b]111\x07\x1b]110\x07");
    let content = term.content();
    assert!(content.background.is_none());
    assert!(content.foreground.is_none());
}

fn searchable() -> Terminal {
    let mut term = Terminal::new(TerminalConfig {
        columns: 20,
        rows: 4,
        scrollback: 100,
        clipboard: zyt_term::ClipboardAccess::CopyPaste,
    })
    .expect("terminal is created");
    term.feed(b"alpha Cat\r\nbeta concat\r\ngamma cat\r\ndelta\r\nepsilon\r\nzeta cat\r\n");
    term
}

fn literal() -> SearchOptions {
    SearchOptions::default()
}

#[test]
fn a_search_selects_the_match_above_the_viewport() {
    let mut term = searchable();
    term.search_set("Cat", literal()).expect("pattern is built");

    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("cat"));
    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("cat"));
    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("cat"));
    assert!(term.display_offset() > 0);
    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("Cat"));
}

#[test]
fn the_case_is_kept_when_it_is_asked_for() {
    let mut term = searchable();
    let options = SearchOptions {
        case_sensitive: true,
        ..literal()
    };
    term.search_set("Cat", options).expect("pattern is built");

    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("Cat"));
    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("Cat"));
}

#[test]
fn a_whole_word_skips_what_a_word_is_part_of() {
    let mut term = searchable();
    let options = SearchOptions {
        kind: SearchKind::Word,
        ..literal()
    };
    term.search_set("cat", options).expect("pattern is built");

    let mut lines = Vec::new();
    for _ in 0..3 {
        assert!(term.search_advance(SearchDirection::Up));
        lines.push(term.selected_text().unwrap_or_default());
    }
    assert_eq!(lines, vec!["cat", "cat", "Cat"]);
}

#[test]
fn a_regular_expression_is_a_pattern() {
    let mut term = searchable();
    let options = SearchOptions {
        kind: SearchKind::Regex,
        case_sensitive: true,
        ..literal()
    };
    term.search_set("c[oa]ncat", options)
        .expect("pattern is built");

    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("concat"));
}

#[test]
fn a_pattern_that_cannot_be_built_is_an_error() {
    let mut term = searchable();
    let options = SearchOptions {
        kind: SearchKind::Regex,
        ..literal()
    };
    let error = term
        .search_set("a(", options)
        .expect_err("pattern is rejected");
    assert!(matches!(error, zyt_term::TermError::InvalidPattern { .. }));
    assert!(!term.search_advance(SearchDirection::Up));
}

#[test]
fn a_search_walks_both_ways_and_wraps_around() {
    let mut term = searchable();
    term.search_set("zeta", literal())
        .expect("pattern is built");

    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("zeta"));
    assert!(term.search_advance(SearchDirection::Down));
    assert_eq!(term.selected_text().as_deref(), Some("zeta"));

    term.search_set("alpha", literal())
        .expect("pattern is built");
    assert!(term.search_advance(SearchDirection::Down));
    assert_eq!(term.selected_text().as_deref(), Some("alpha"));
}

#[test]
fn a_query_that_is_nowhere_finds_nothing() {
    let mut term = searchable();
    term.search_set("omega", literal())
        .expect("pattern is built");
    assert!(!term.search_advance(SearchDirection::Up));
    assert!(term.selected_text().is_none());
}

#[test]
fn every_match_of_the_screen_is_marked_when_it_is_asked_for() {
    let mut term = terminal();
    term.feed(b"cat here\r\nand cat\r\n");
    let options = SearchOptions {
        highlight_all: true,
        ..literal()
    };
    term.search_set("cat", options).expect("pattern is built");

    let content = term.content();
    let marked: usize = content.cells.iter().filter(|cell| cell.matched).count();
    assert_eq!(marked, 6);

    term.search_set("cat", literal()).expect("pattern is built");
    let content = term.content();
    assert!(content.cells.iter().all(|cell| !cell.matched));
}

#[test]
fn a_marked_match_answers_the_whole_word_as_well() {
    let mut term = terminal();
    term.feed(b"cat concat\r\n");
    let options = SearchOptions {
        kind: SearchKind::Word,
        highlight_all: true,
        ..literal()
    };
    term.search_set("cat", options).expect("pattern is built");

    let content = term.content();
    let marked: usize = content.cells.iter().filter(|cell| cell.matched).count();
    assert_eq!(marked, 3);
}

#[test]
fn clearing_the_search_takes_the_selection_with_it() {
    let mut term = searchable();
    term.search_set("cat", literal()).expect("pattern is built");
    assert!(term.search_advance(SearchDirection::Up));
    assert!(term.selected_text().is_some());

    term.search_clear();
    assert!(term.selected_text().is_none());
    assert!(!term.search_advance(SearchDirection::Up));
}

#[test]
fn a_fuzzy_query_spans_from_its_first_character_to_its_last() {
    let mut term = searchable();
    let options = SearchOptions {
        kind: SearchKind::Fuzzy,
        case_sensitive: true,
        ..literal()
    };
    term.search_set("bc", options).expect("pattern is built");

    assert!(term.search_advance(SearchDirection::Up));
    assert_eq!(term.selected_text().as_deref(), Some("beta conc"));
}

#[test]
fn a_fuzzy_query_still_needs_its_characters_in_order() {
    let mut term = searchable();
    let options = SearchOptions {
        kind: SearchKind::Fuzzy,
        case_sensitive: true,
        ..literal()
    };
    term.search_set("zq", options).expect("pattern is built");

    assert!(!term.search_advance(SearchDirection::Up));
}

/// Filling the scrollback and then asking for less keeps the newest lines and
/// drops the rest, which is what a window told to hold less memory does.
#[test]
fn a_smaller_scrollback_drops_the_oldest_lines() {
    let mut term = terminal();
    for index in 0..60 {
        term.feed(format!("line {index}\r\n").as_bytes());
    }
    assert_eq!(term.history_size(), 57);

    term.set_scrollback(10);

    assert_eq!(term.scrollback(), 10);
    assert_eq!(term.history_size(), 10);
    term.scroll(10);
    let content = term.content();
    assert_eq!(
        line_text(&content, 0).trim_end(),
        "line 47",
        "the oldest line left is the tenth above the screen"
    );
}

/// The scrollback of a session that is over is dropped whole, and the grid may
/// fill again up to the same cap.
#[test]
fn a_forgotten_scrollback_leaves_the_screen_and_the_cap() {
    let mut term = terminal();
    for index in 0..60 {
        term.feed(format!("line {index}\r\n").as_bytes());
    }

    term.forget_scrollback();

    assert_eq!(term.history_size(), 0, "nothing above the screen is left");
    assert_eq!(term.scrollback(), 100, "the cap is the one it had");
    let content = term.content();
    assert_eq!(
        line_text(&content, 2).trim_end(),
        "line 59",
        "the screen is untouched"
    );

    for index in 60..80 {
        term.feed(format!("line {index}\r\n").as_bytes());
    }
    assert_eq!(term.history_size(), 20, "and it fills again");
}

/// The size of a selection is counted over the text it would copy, so the
/// widest line and not the width of the grid is what the columns say, and the
/// line breaks are not characters.
#[test]
fn the_size_of_a_selection_counts_the_text_it_would_copy() {
    let mut term = terminal();
    term.feed(b"abcdef\r\nxy\r\n");

    assert_eq!(term.selection_size(), None, "nothing is selected");

    term.selection_start(SelectionKind::Simple, 0, 0)
        .expect("position is inside the grid");
    term.selection_update(1, 1)
        .expect("position is inside the grid");

    let size = term.selection_size().expect("the selection has a size");
    assert_eq!(size.lines, 2);
    assert_eq!(size.columns, 6, "the longest line of the two");
    assert_eq!(size.characters, 8, "the line break is not a character");

    term.selection_clear();
    assert_eq!(term.selection_size(), None, "the selection was let go of");
}

/// The count is kept between frames, and bytes that may have changed the text
/// under the selection take it down again.
#[test]
fn the_size_of_a_selection_follows_what_arrives() {
    let mut term = terminal();
    term.feed(b"ab");
    term.selection_start(SelectionKind::Simple, 0, 0)
        .expect("position is inside the grid");
    term.selection_update(1, 0)
        .expect("position is inside the grid");

    let size = term.selection_size().expect("the selection has a size");
    assert_eq!(size.characters, 2);
    assert_eq!(term.selection_size(), Some(size), "the same answer twice");

    term.feed(b"\rxyz");
    let size = term.selection_size().expect("the selection has a size");
    assert_eq!(size.characters, 2, "the selection still spans two cells");
    assert_eq!(size.columns, 2);
}

/// The character that was pressed on is in the selection, and so is the one the
/// pointer ends on: what is pointed at is taken whole, at both ends.
#[test]
fn a_selection_takes_the_characters_at_both_of_its_ends() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.selection_start(SelectionKind::Simple, 1, 0)
        .expect("position is inside the grid");
    term.selection_update(2, 0)
        .expect("position is inside the grid");

    assert_eq!(
        term.selected_text().as_deref(),
        Some("bc"),
        "the character pressed on is the first of the selection"
    );
}

/// The same selection made the other way round takes the same two characters.
///
/// The one that was pressed on is in it whichever way it grows: it is what was
/// pointed at, and a selection that dropped it when the pointer went left would
/// take a different pair of characters for the same pair of places.
#[test]
fn a_selection_dragged_left_keeps_the_character_it_started_on() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.selection_start(SelectionKind::Simple, 2, 0)
        .expect("position is inside the grid");
    term.selection_update(1, 0)
        .expect("position is inside the grid");

    assert_eq!(term.selected_text().as_deref(), Some("bc"));
}

/// A selection that has not moved off the character it started on is that one
/// character: a press and a pointer standing in the same cell name the same
/// character, and it is in the selection.
#[test]
fn a_selection_that_moved_nowhere_is_the_one_character_it_began_on() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.selection_start(SelectionKind::Simple, 3, 0)
        .expect("position is inside the grid");
    term.selection_update(3, 0)
        .expect("position is inside the grid");

    assert_eq!(term.selected_text().as_deref(), Some("d"));
}

/// The anchor says where a selection would begin. It is a place in the text
/// and not on the screen, so it walks with the text when the page scrolls: it
/// leaves the page with the line it stands on and comes back with it.
#[test]
fn the_anchor_walks_with_the_text_it_was_put_in() {
    let mut term = terminal();
    term.feed(b"abcdef");

    assert_eq!(term.content().selection_anchor, None, "nothing said yet");

    term.set_selection_anchor(2, 0)
        .expect("position is inside the grid");
    assert_eq!(term.content().selection_anchor, Some((2, 0)));

    // Four rows: the page is filled and nothing has moved yet.
    for line in 1..4 {
        term.feed(format!("\r\nline{line}").as_bytes());
    }
    assert_eq!(term.content().selection_anchor, Some((2, 0)));

    // One line more, and the line it stands on is in the history.
    term.feed(b"\r\nlast");
    assert_eq!(
        term.content().selection_anchor,
        None,
        "the page it stood on is not the page any more"
    );

    term.scroll(1);
    assert_eq!(
        term.content().selection_anchor,
        Some((2, 0)),
        "and the page scrolled back to it shows it where the text is"
    );

    term.clear_selection_anchor();
    assert_eq!(term.content().selection_anchor, None);
}

/// Starting a selection is saying where it begins, so the anchor goes there
/// too: what the bar says and what a drag would take are one answer.
#[test]
fn starting_a_selection_puts_the_anchor_where_it_starts() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.selection_start(SelectionKind::Simple, 3, 0)
        .expect("position is inside the grid");

    assert_eq!(term.content().selection_anchor, Some((3, 0)));

    term.selection_clear();
    assert_eq!(
        term.content().selection_anchor,
        Some((3, 0)),
        "a selection let go of does not move where the next one would begin"
    );
}

/// With a place marked and nothing selected, extending selects from that place
/// to where the press landed — on either side of it, and the same way.
#[test]
fn extending_from_the_anchor_selects_up_to_the_press() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.set_selection_anchor(2, 0)
        .expect("position is inside the grid");
    term.selection_extend(4, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("cde"));

    let mut term = terminal();
    term.feed(b"abcdef");
    term.set_selection_anchor(4, 0)
        .expect("position is inside the grid");
    term.selection_extend(2, 0)
        .expect("position is inside the grid");
    assert_eq!(
        term.selected_text().as_deref(),
        Some("cde"),
        "a press before the place it began at selects back to it, that place in"
    );
}

/// With a selection running, extending keeps the end further from the press and
/// moves the nearer one to it: a press outside the selection grows it, one
/// inside brings the near end in, and neither turns it over.
#[test]
fn extending_a_selection_grows_it_and_shrinks_it() {
    let mut term = terminal();
    term.feed(b"abcdefgh");

    term.selection_start(SelectionKind::Simple, 1, 0)
        .expect("position is inside the grid");
    term.selection_update(2, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("bc"));

    term.selection_extend(5, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("bcdef"), "grown");

    term.selection_extend(3, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("bcd"), "shrunk");

    term.selection_extend(0, 0)
        .expect("position is inside the grid");
    assert_eq!(
        term.selected_text().as_deref(),
        Some("abcd"),
        "a press on the other side of where it began grows it that way and keeps \
         what it had, rather than turning the selection over"
    );
}

/// Every press with `Shift` held puts the anchor on the end further from it, and
/// the selection then runs from that end to the press.
///
/// Which end is further is asked again on every press, so a press that moved one
/// end moved what the next press is measured against.
#[test]
fn extending_puts_the_anchor_on_the_end_further_from_the_press() {
    let mut term = terminal();
    term.feed(b"abcdefgh");

    term.selection_start(SelectionKind::Simple, 1, 0)
        .expect("position is inside the grid");
    term.selection_update(6, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("bcdefg"));

    // `c` is one character from `b` and four from `g`, so the anchor goes to `g`
    // and the near end comes in to the press.
    term.selection_extend(2, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("cdefg"));
    assert_eq!(
        term.content().selection_anchor,
        Some((6, 0)),
        "the mark stands on the end that stayed"
    );

    // `d` is one from `c` and three from `g`: the same end stays, and the
    // selection cuts back again.
    term.selection_extend(3, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("defg"));

    // `f` is three from `d` and one from `g`, so the other end is the far one now
    // and the selection cuts back from the right.
    term.selection_extend(5, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("def"));
    assert_eq!(term.content().selection_anchor, Some((3, 0)));
}

/// The two ends are what a press is measured against, and not a point between
/// them.
///
/// A press on the character just before the middle of `bcdefg` is two characters
/// from one end and three from the other, so the far end stays and the near end
/// comes in — where a midpoint counted in whole characters would have named that
/// very character the middle and moved the other end instead.
#[test]
fn extending_measures_the_press_against_the_ends_and_not_their_midpoint() {
    let mut term = terminal();
    term.feed(b"abcdefgh");

    term.selection_start(SelectionKind::Simple, 1, 0)
        .expect("position is inside the grid");
    term.selection_update(6, 0)
        .expect("position is inside the grid");

    term.selection_extend(3, 0)
        .expect("position is inside the grid");

    assert_eq!(term.selected_text().as_deref(), Some("defg"));
}

/// The end that stayed is where a drag begun with `Shift` held grows from, so a
/// press that adds to a selection and a pointer that keeps moving are one
/// gesture.
#[test]
fn a_drag_that_began_with_shift_grows_from_the_end_that_stayed() {
    let mut term = terminal();
    term.feed(b"abcdefgh");

    term.selection_start(SelectionKind::Simple, 5, 0)
        .expect("position is inside the grid");
    term.selection_update(6, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("fg"));

    term.selection_extend(1, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("bcdefg"));

    term.selection_update(3, 0)
        .expect("position is inside the grid");
    assert_eq!(
        term.selected_text().as_deref(),
        Some("defg"),
        "the drag grows from `g`, the end the press left alone"
    );
}

/// A press with `Shift` held adds characters and not words, whatever the
/// selection was picked out by.
///
/// A double press takes a word because a word is what was pointed at; a press
/// that adds to it is aimed at a character, and a selection that went on walking
/// by words would take in what nobody pointed at.
#[test]
fn extending_a_word_selection_grows_it_by_characters() {
    let mut term = terminal();
    term.feed(b"one two three");

    term.selection_start(SelectionKind::Semantic, 5, 0)
        .expect("position is inside the grid");
    assert_eq!(term.selected_text().as_deref(), Some("two"), "the word");

    term.selection_extend(9, 0)
        .expect("position is inside the grid");
    assert_eq!(
        term.selected_text().as_deref(),
        Some("two th"),
        "and the characters up to the press, not the word they stand in"
    );
}

/// Nothing marked and nothing selected is nothing to extend: a press that
/// would otherwise select from wherever the page happens to start is a press
/// that selects what nobody asked for.
#[test]
fn extending_nothing_selects_nothing() {
    let mut term = terminal();
    term.feed(b"abcdef");

    term.selection_extend(3, 0)
        .expect("position is inside the grid");

    assert_eq!(term.selected_text(), None);
}

/// A program that paints over the color table is carried in the snapshot, by
/// the index it painted, and a table nobody painted is empty rather than 256
/// entries of nothing.
#[test]
fn the_colors_a_program_paints_reach_the_snapshot() {
    let mut term = terminal();
    assert!(
        term.content().palette.is_empty(),
        "nothing was painted over yet"
    );

    term.feed(b"\x1b]4;1;rgb:ff/00/7f\x07");
    let content = term.content();
    assert_eq!(content.palette.len(), zyt_term::PALETTE_COLORS);
    assert_eq!(
        content.palette[1],
        Some(zyt_term::Rgb {
            r: 0xff,
            g: 0x00,
            b: 0x7f
        })
    );
    assert_eq!(content.palette[2], None, "the rest is left alone");
}

/// A color the program gives back is the color of the theme again, and giving
/// back the last one it held leaves the table empty.
#[test]
fn a_color_given_back_is_no_longer_painted() {
    let mut term = terminal();
    term.feed(b"\x1b]4;1;rgb:ff/00/7f\x07\x1b]4;2;rgb:00/ff/00\x07");
    assert!(term.content().palette[2].is_some());

    term.feed(b"\x1b]104;2\x07");
    let content = term.content();
    assert_eq!(content.palette[2], None);
    assert!(content.palette[1].is_some(), "the other one still stands");

    term.feed(b"\x1b]104;1\x07");
    assert!(
        term.content().palette.is_empty(),
        "nothing is painted over any more"
    );
}

/// The cursor color a program asks for is carried too, and giving it back
/// leaves the cursor to the theme again.
#[test]
fn the_cursor_color_a_program_asks_for_is_reported() {
    let mut term = terminal();
    assert_eq!(term.content().cursor_color, None);

    term.feed(b"\x1b]12;rgb:00/ff/00\x07");
    assert_eq!(
        term.content().cursor_color,
        Some(zyt_term::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00
        })
    );

    term.feed(b"\x1b]112\x07");
    assert_eq!(term.content().cursor_color, None);
}

/// A run of NUL bytes reaches the grid as one block: the mark, the sign and the
/// count of the bytes, standing where the bytes stood.
///
/// The parser of a terminal drops a NUL byte, so nothing else would be there to
/// see — and a device that has gone quiet in the middle of a word is exactly
/// what somebody watching a line is looking for.
#[test]
fn a_run_of_nul_bytes_is_a_block_of_the_grid() {
    let mut term = terminal();
    term.feed(b"a\0\0\0b");
    let content = term.content();

    assert_eq!(zyt_term::null_part(content.cell(0, 0).unwrap().ch), None);
    assert_eq!(
        zyt_term::null_part(content.cell(1, 0).unwrap().ch),
        Some(zyt_term::NullPart::Mark)
    );
    assert_eq!(
        zyt_term::null_part(content.cell(2, 0).unwrap().ch),
        Some(zyt_term::NullPart::Times)
    );
    assert_eq!(
        zyt_term::null_part(content.cell(3, 0).unwrap().ch),
        Some(zyt_term::NullPart::Digit(3))
    );
    assert_eq!(content.cell(4, 0).unwrap().ch, 'b');
}

/// One NUL byte is one cell: the mark, and no count beside it.
#[test]
fn one_nul_byte_takes_one_cell() {
    let mut term = terminal();
    term.feed(b"a\0b");
    let content = term.content();

    assert_eq!(
        zyt_term::null_part(content.cell(1, 0).unwrap().ch),
        Some(zyt_term::NullPart::Mark)
    );
    assert_eq!(
        content.cell(2, 0).unwrap().ch,
        'b',
        "the count of one is not written"
    );
}

/// The text of a selection carries the block as the characters it was drawn as,
/// so what was read on the screen is what is pasted.
#[test]
fn a_block_is_copied_as_what_it_was_drawn_as() {
    let mut term = terminal();
    term.feed(b"\0\0");
    term.selection_start(SelectionKind::Lines, 0, 0)
        .expect("the line is on the page");
    term.selection_update(1, 0)
        .expect("the line is on the page");

    let text = term.selected_text().expect("the line is selected");

    assert!(
        text.starts_with(&format!(
            "{}{}2",
            zyt_term::NULL_SYMBOL,
            zyt_term::TIMES_SIGN
        )),
        "{text:?}"
    );
}

/// Answers the terminal has not handed over can be thrown away.
///
/// They are replies to what a program asked, so dropping them leaves it waiting —
/// which is what a caller giving up on everything on its way to the device asks
/// for, these bytes being on their way too.
#[test]
fn the_answers_not_handed_over_yet_can_be_given_up_on() {
    let mut term = terminal();
    term.feed(b"\x1b[6n");
    assert!(!term.take_output().is_empty(), "a query is answered at all");

    term.feed(b"\x1b[6n");
    term.forget_output();

    assert!(
        term.take_output().is_empty(),
        "and the answer is gone once it is given up on"
    );
}

#[test]
fn a_block_selection_of_the_keyboard_begins_at_the_cursor() {
    let mut term = terminal();
    term.feed(b"abcd\r\nefgh");
    // The cursor stands past `h`, so one step to the left takes that cell and
    // the one the cursor is on.
    term.select_by_key(SelectionStep::Left);
    assert!(term.key_selecting());
    assert_eq!(term.selected_text().as_deref(), Some("h"));
}

#[test]
fn every_step_grows_the_block_and_the_anchor_stays() {
    let mut term = terminal();
    term.feed(b"abcd\r\nefgh");
    term.select_by_key(SelectionStep::Left);
    term.select_by_key(SelectionStep::Left);
    term.select_by_key(SelectionStep::Up);
    // Two columns of two rows: the block the keys drew, and not the run of text
    // between its corners.
    assert_eq!(term.selected_text().as_deref(), Some("cd\ngh"));
}

#[test]
fn a_step_against_an_edge_of_the_grid_moves_the_other_end() {
    let mut term = terminal();
    // The carriage return leaves the cursor in the first cell of a written
    // row, so the caret has nowhere to walk from the very first step.
    term.feed(b"abcdefghij\r");
    term.select_by_key(SelectionStep::Left);
    assert_eq!(term.selected_text().as_deref(), Some("ab"));

    // Every later step is the same: the caret stays against the edge and the
    // block goes on growing from the end that can still move.
    term.select_by_key(SelectionStep::Left);
    term.select_by_key(SelectionStep::Left);
    assert_eq!(term.selected_text().as_deref(), Some("abcd"));
}

#[test]
fn an_edge_above_or_below_is_where_a_step_stops() {
    let mut term = terminal();
    term.feed(b"one\r\ntwo\r\nthree");
    for _ in 0..3 {
        term.select_by_key(SelectionStep::Up);
    }
    let reached = term.selected_text();
    // The oldest line is the end of it: nothing grows downwards for a key that
    // asked to go up.
    for _ in 0..10 {
        term.select_by_key(SelectionStep::Up);
    }
    assert_eq!(term.selected_text(), reached);
}

#[test]
fn the_ends_of_a_row_are_the_ends_the_keys_walk_to() {
    let mut term = terminal();
    term.feed(b"one\r\ntwo\r\nthree");
    // The cursor stands past `three`, so the start of its row takes that row
    // from its first cell to where the cursor is, and nothing of the rows above.
    term.select_by_key(SelectionStep::LineStart);
    assert_eq!(term.selected_text().as_deref(), Some("three"));

    // The other end of the row is the last cell of it, and the step stays on
    // that row: what lies between the anchor and the right edge is blank, and
    // no line of the rows above or below is taken.
    term.select_by_key(SelectionStep::LineEnd);
    let text = term.selected_text().expect("a selection stands");
    assert!(!text.contains('\n'), "{text:?} left the row");
}

#[test]
fn the_ends_of_a_row_carry_a_selection_that_stands() {
    let mut term = terminal();
    term.feed(b"one\r\ntwo\r\nthree");
    term.select_by_key(SelectionStep::Up);
    term.select_by_key(SelectionStep::LineStart);
    // Two rows, each from its first cell to the column the gesture reached.
    assert_eq!(
        term.selected_text().as_deref(),
        Some("two\nthree".trim_start())
    );
}

#[test]
fn a_selection_of_the_keyboard_is_let_go_of_in_one_call() {
    let mut term = terminal();
    term.feed(b"abcd");
    term.select_by_key(SelectionStep::Left);
    assert!(term.key_selecting());

    term.forget_key_selection();
    assert!(!term.key_selecting());
    assert_eq!(term.selected_text(), None);
}

#[test]
fn picking_out_a_selection_is_a_state_of_the_terminal() {
    let mut term = terminal();
    term.feed(b"abcd");
    assert!(!term.selecting());

    term.select_by_key(SelectionStep::Left);
    assert!(term.selecting(), "the keys began one");
    term.forget_key_selection();
    assert!(!term.selecting());

    term.selection_start(SelectionKind::Simple, 1, 0)
        .expect("a press on the page");
    assert!(term.selecting(), "a press began one as readily");
    assert!(!term.key_selecting(), "but not with the keys");
    term.selection_clear();
    assert!(!term.selecting());
}
