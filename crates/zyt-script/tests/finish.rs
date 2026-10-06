//! The key a script sends when it is over.

use zyt_script::{FINISH_NONE, FINISH_PRESETS, ScriptError, finish_bytes, finish_label};

#[test]
fn the_finish_key_has_a_readable_text_form() {
    let named = [
        ("esc", vec![0x1b]),
        ("Escape", vec![0x1b]),
        ("enter", vec![0x0d]),
        ("lf", vec![0x0a]),
        ("tab", vec![0x09]),
        ("ctrl+c", vec![0x03]),
        ("ctrl+d", vec![0x04]),
        ("ctrl+[", vec![0x1b]),
        (r"\x03", vec![0x03]),
        (r"\r\n", vec![0x0d, 0x0a]),
        ("quit", b"quit".to_vec()),
        (FINISH_NONE, Vec::new()),
    ];

    for (text, expected) in named {
        assert_eq!(
            finish_bytes(text).expect("the key is valid"),
            expected,
            "{text}"
        );
    }
}

#[test]
fn a_broken_finish_key_is_reported() {
    for text in ["ctrl+", "ctrl+shift", r"\x0", r"\q"] {
        let error = finish_bytes(text).expect_err("the key is broken");
        assert!(
            matches!(error, ScriptError::InvalidFinishKey { .. }),
            "{text}"
        );
    }
}

#[test]
fn every_offered_key_is_one_that_parses() {
    for text in FINISH_PRESETS {
        let bytes = finish_bytes(text).expect("an offered key is valid");
        assert_eq!(bytes.len(), 1, "{text}");
    }
}

#[test]
fn a_key_that_sends_nothing_has_nothing_to_say_about_itself() {
    assert_eq!(finish_label(FINISH_NONE), None);
    assert_eq!(finish_label("  "), None);
    assert_eq!(finish_label(" ctrl+c "), Some("ctrl+c".to_string()));
}
