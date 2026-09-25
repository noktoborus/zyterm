//! Resolution of key presses, key map files and palette search.

use zyt_keymux::{
    CONTEXT_PALETTE, CONTEXT_SEARCH, CONTEXT_TERMINAL, Chord, Command, CommandId, CommandRegistry,
    Context, Dispatch, KeyCode, KeyDispatcher, KeyStroke, Keymap, KeymapError, Modifiers,
    default_keymap,
};

fn stroke(text: &str) -> KeyStroke {
    KeyStroke::parse(text).expect("valid key")
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
    let mut keymap = Keymap::new();
    keymap
        .bind(CONTEXT_TERMINAL, "escape", "terminal.escape")
        .unwrap();
    keymap
        .bind(CONTEXT_PALETTE, "escape", "palette.close")
        .unwrap();

    let mut dispatcher = KeyDispatcher::new(keymap);
    dispatcher.set_contexts(&[CONTEXT_TERMINAL]);
    assert_eq!(
        dispatcher.press(stroke("escape")),
        Dispatch::Command(CommandId::new("terminal.escape"))
    );

    dispatcher.set_contexts(&[CONTEXT_PALETTE]);
    assert_eq!(
        dispatcher.press(stroke("escape")),
        Dispatch::Command(CommandId::new("palette.close"))
    );
}

#[test]
fn sequences_need_every_key() {
    let mut keymap = Keymap::new();
    keymap
        .bind("global", "ctrl+k ctrl+s", "keymap.edit")
        .unwrap();
    let mut dispatcher = KeyDispatcher::new(keymap);

    assert_eq!(dispatcher.press(stroke("ctrl+k")), Dispatch::Pending);
    assert_eq!(dispatcher.pending().len(), 1);
    assert_eq!(
        dispatcher.press(stroke("ctrl+s")),
        Dispatch::Command(CommandId::new("keymap.edit"))
    );
    assert!(dispatcher.pending().is_empty());

    assert_eq!(dispatcher.press(stroke("ctrl+k")), Dispatch::Pending);
    assert_eq!(dispatcher.press(stroke("x")), Dispatch::Unhandled);
}

#[test]
fn unbound_keys_are_left_to_the_caller() {
    let mut dispatcher = KeyDispatcher::new(default_keymap().unwrap());
    dispatcher.set_contexts(&[CONTEXT_TERMINAL]);
    let press = KeyStroke {
        code: KeyCode::Char('a'),
        modifiers: Modifiers::default(),
    };
    assert_eq!(dispatcher.press(press), Dispatch::Unhandled);
}

#[test]
fn conflicting_binding_is_rejected() {
    let mut keymap = Keymap::new();
    keymap.bind("global", "ctrl+p", "one").unwrap();
    let error = keymap
        .bind("global", "ctrl+p", "two")
        .expect_err("keys are taken");
    assert!(matches!(error, KeymapError::Conflict { .. }));
}

#[test]
fn user_map_overrides_the_default_map() {
    let mut keymap = default_keymap().unwrap();
    let user = Keymap::from_yaml(
        "contexts:\n  terminal:\n    - keys: ctrl+shift+c\n      command: terminal.clear\n",
    )
    .expect("user map parses");
    keymap.merge(user);

    let contexts = [Context::new(CONTEXT_TERMINAL)];
    assert_eq!(
        keymap.command_for(&contexts, &[stroke("ctrl+shift+c")]),
        Some(CommandId::new("terminal.clear"))
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

    let open = Chord::parse("ctrl+shift+f").expect("the chord parses");
    let down = Chord::parse("ctrl+f").expect("the chord parses");
    let accept = Chord::parse("enter").expect("the chord parses");
    let leave = Chord::parse("escape").expect("the chord parses");

    assert_eq!(
        keymap.command_for(&terminal, &open.strokes),
        Some(CommandId::new("search.open"))
    );
    assert_eq!(keymap.command_for(&terminal, &down.strokes), None);
    assert_eq!(
        keymap.command_for(&search, &open.strokes),
        Some(CommandId::new("search.previous"))
    );
    assert_eq!(keymap.command_for(&search, &accept.strokes), None);
    assert_eq!(
        keymap.command_for(&search, &down.strokes),
        Some(CommandId::new("search.next"))
    );
    assert_eq!(
        keymap.command_for(&search, &leave.strokes),
        Some(CommandId::new("search.close"))
    );
}
