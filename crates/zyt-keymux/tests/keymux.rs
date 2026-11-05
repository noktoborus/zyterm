//! Resolution of key presses, key map files and palette search.

use std::time::{Duration, Instant};
use zyt_keymux::{
    Binding, CONTEXT_PALETTE, CONTEXT_SEARCH, CONTEXT_TERMINAL, CONTEXT_TMUX, Chord, Command,
    CommandId, CommandRegistry, Context, Dispatch, KeyDispatcher, KeyStroke, Keymap, KeymapError,
    REPEAT_TIME, default_keymap,
};

fn stroke(text: &str) -> KeyStroke {
    KeyStroke::parse(text).expect("valid key")
}

fn command(id: &str) -> Dispatch {
    Dispatch::Command(CommandId::new(id))
}

fn keymap(bindings: &[(&str, &str, &str, bool)]) -> Keymap {
    let mut keymap = Keymap::new();
    for (context, keys, id, repeat) in bindings {
        let binding = Binding::parse(keys, id, *repeat).expect("valid binding");
        keymap.bind(context, binding).expect("no conflict");
    }
    keymap
}

/// A dispatcher of a prefix, a key after it and two keys that repeat.
fn tmux() -> KeyDispatcher {
    let mut dispatcher = KeyDispatcher::new(keymap(&[
        ("global", "ctrl+b c", "new", false),
        ("global", "ctrl+b %", "split", false),
        ("global", "ctrl+b up", "focus", true),
        ("global", "ctrl+b ctrl+up", "resize", true),
    ]));
    dispatcher.set_repeat_time(Duration::from_millis(500));
    dispatcher
}

#[test]
fn default_layout_is_valid() {
    let keymap = default_keymap().expect("default layout parses");
    assert!(keymap.conflicts().is_empty());
    let yaml = keymap.to_yaml().expect("layout serializes");
    let parsed = Keymap::from_yaml(&yaml).expect("layout parses back");
    assert_eq!(parsed, keymap);
}

#[test]
fn context_decides_which_binding_is_used() {
    let now = Instant::now();
    let mut dispatcher = KeyDispatcher::new(keymap(&[
        (CONTEXT_TERMINAL, "escape", "terminal.escape", false),
        (CONTEXT_PALETTE, "escape", "palette.close", false),
    ]));
    dispatcher.set_contexts(&[CONTEXT_TERMINAL]);
    assert_eq!(
        dispatcher.press(stroke("escape"), now),
        command("terminal.escape")
    );

    dispatcher.set_contexts(&[CONTEXT_PALETTE]);
    assert_eq!(
        dispatcher.press(stroke("escape"), now),
        command("palette.close")
    );
}

#[test]
fn a_sequence_waits_for_every_key_and_a_key_that_breaks_it_is_dropped() {
    let now = Instant::now();
    let mut dispatcher = tmux();

    assert_eq!(dispatcher.press(stroke("ctrl+b"), now), Dispatch::Pending);
    let later = now + Duration::from_secs(60);
    assert_eq!(
        dispatcher.press(stroke("%"), later),
        command("split"),
        "it waits without a limit, and a character is bound as itself"
    );
    assert!(dispatcher.pending().is_empty());

    assert_eq!(dispatcher.press(stroke("ctrl+b"), now), Dispatch::Pending);
    assert_eq!(dispatcher.press(stroke("escape"), now), Dispatch::Dropped);
    assert_eq!(
        dispatcher.press(stroke("c"), now),
        Dispatch::Unhandled,
        "the key after it is the caller's again"
    );
}

#[test]
fn what_goes_on_a_sequence_is_offered_with_the_keys_left() {
    let now = Instant::now();
    let mut dispatcher = tmux();
    assert!(dispatcher.next(now).is_empty());

    dispatcher.press(stroke("ctrl+b"), now);
    let next: Vec<String> = dispatcher
        .next(now)
        .iter()
        .map(|(rest, binding)| format!("{} {}", rest[0], binding.command))
        .collect();
    assert_eq!(next, ["c new", "% split", "up focus", "ctrl+up resize"]);
}

#[test]
fn a_binding_that_repeats_runs_by_its_last_key_until_a_pause() {
    let now = Instant::now();
    let step = Duration::from_millis(400);
    let mut dispatcher = tmux();

    dispatcher.press(stroke("ctrl+b"), now);
    assert_eq!(dispatcher.press(stroke("ctrl+up"), now), command("resize"));
    assert_eq!(
        dispatcher.press(stroke("ctrl+up"), now + step),
        command("resize")
    );
    assert_eq!(
        dispatcher.press(stroke("up"), now + step * 2),
        command("focus"),
        "another key that repeats after the same prefix, each press later"
    );
    let offered: Vec<&str> = dispatcher
        .next(now + step * 2)
        .iter()
        .map(|(_, binding)| binding.command.as_str())
        .collect();
    assert_eq!(offered, ["focus", "resize"], "only what repeats");

    assert_eq!(
        dispatcher.press(stroke("up"), now + step * 2 + REPEAT_TIME * 2),
        Dispatch::Unhandled,
        "past the pause the key is the caller's"
    );
}

#[test]
fn a_key_that_does_not_repeat_ends_the_repeat_and_is_the_callers() {
    let now = Instant::now();
    let mut dispatcher = tmux();
    dispatcher.press(stroke("ctrl+b"), now);
    dispatcher.press(stroke("up"), now);
    assert!(dispatcher.repeating(now).is_some());

    assert_eq!(dispatcher.press(stroke("c"), now), Dispatch::Unhandled);
    assert!(dispatcher.repeating(now).is_none());
}

#[test]
fn a_binding_that_does_not_repeat_starts_no_repeat() {
    let now = Instant::now();
    let mut dispatcher = tmux();
    dispatcher.press(stroke("ctrl+b"), now);
    assert_eq!(dispatcher.press(stroke("c"), now), command("new"));
    assert!(dispatcher.repeating(now).is_none());
}

#[test]
fn unbound_keys_are_left_to_the_caller() {
    let mut dispatcher = KeyDispatcher::new(default_keymap().unwrap());
    dispatcher.set_contexts(&[CONTEXT_TERMINAL]);
    assert_eq!(
        dispatcher.press(stroke("a"), Instant::now()),
        Dispatch::Unhandled
    );
}

#[test]
fn the_prefix_of_tmux_is_the_callers_outside_tmux() {
    let now = Instant::now();
    let mut dispatcher = KeyDispatcher::new(default_keymap().unwrap());
    dispatcher.set_contexts(&[CONTEXT_TERMINAL]);
    assert_eq!(dispatcher.press(stroke("ctrl+b"), now), Dispatch::Unhandled);

    dispatcher.set_contexts(&[CONTEXT_TMUX, CONTEXT_TERMINAL]);
    assert_eq!(dispatcher.press(stroke("ctrl+b"), now), Dispatch::Pending);
    assert_eq!(
        dispatcher.press(stroke("c"), now),
        command("tmux.window.new")
    );
}

#[test]
fn conflicting_binding_is_rejected() {
    let mut keymap = keymap(&[("global", "ctrl+p", "one", false)]);
    let error = keymap
        .bind("global", Binding::parse("ctrl+p", "two", false).unwrap())
        .expect_err("keys are taken");
    assert!(matches!(error, KeymapError::Conflict { .. }));
}

#[test]
fn user_map_overrides_the_default_map() {
    let mut keymap = default_keymap().unwrap();
    let user = Keymap::from_yaml(
        "contexts:\n  terminal:\n    - keys: ctrl+shift+c\n      command: terminal.clear\n  tmux:\n    - keys: ctrl+a up\n      command: tmux.pane.focus_up\n      repeat: true\n",
    )
    .expect("user map parses");
    keymap.merge(user);

    let contexts = [Context::new(CONTEXT_TERMINAL)];
    assert_eq!(
        keymap.command_for(&contexts, &[stroke("ctrl+shift+c")]),
        Some(CommandId::new("terminal.clear"))
    );
    let tmux = [Context::new(CONTEXT_TMUX)];
    let keys = Chord::parse("ctrl+a up").unwrap();
    assert!(
        keymap
            .find(&tmux, &keys.strokes)
            .is_some_and(|binding| binding.repeat)
    );
}

#[test]
fn palette_search_ranks_and_filters() {
    let mut registry = CommandRegistry::new();
    registry.add(Command {
        id: CommandId::new("terminal.copy"),
        context: Context::new(CONTEXT_TERMINAL),
        title: "Copy selection".to_string(),
    });
    registry.add(Command {
        id: CommandId::new("transfer.send_file"),
        context: Context::new(CONTEXT_TERMINAL),
        title: "Send file".to_string(),
    });
    registry.add(Command {
        id: CommandId::new("settings.close"),
        context: Context::new("settings"),
        title: "Close settings".to_string(),
    });

    let keymap = default_keymap().unwrap();
    let contexts = [Context::new(CONTEXT_TERMINAL)];

    let all = registry.search("", &contexts, &keymap);
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|hit| hit.keys.is_some()));

    let hits = registry.search("sendf", &contexts, &keymap);
    assert_eq!(
        hits.first().unwrap().command.id,
        CommandId::new("transfer.send_file")
    );
}

#[test]
fn the_search_answers_its_own_keys_and_leaves_the_terminal_its_own() {
    let keymap = default_keymap().expect("the default layout is valid");
    let terminal = [Context::new(CONTEXT_TERMINAL)];
    let search = [Context::new(CONTEXT_SEARCH)];
    let keys = |text: &str| Chord::parse(text).expect("the chord parses").strokes;

    assert_eq!(
        keymap.command_for(&terminal, &keys("ctrl+shift+f")),
        Some(CommandId::new("search.open"))
    );
    assert_eq!(keymap.command_for(&terminal, &keys("ctrl+f")), None);
    assert_eq!(
        keymap.command_for(&search, &keys("ctrl+shift+f")),
        Some(CommandId::new("search.previous"))
    );
    assert_eq!(keymap.command_for(&search, &keys("enter")), None);
    assert_eq!(
        keymap.command_for(&search, &keys("ctrl+f")),
        Some(CommandId::new("search.next"))
    );
    assert_eq!(
        keymap.command_for(&search, &keys("escape")),
        Some(CommandId::new("search.close"))
    );
}
