//! The keys a script names to `zyt.line.write_key`.

use zyt_script::{ScriptError, key_bytes};

#[test]
fn a_key_has_a_readable_text_form() {
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
        ("", Vec::new()),
    ];

    for (text, expected) in named {
        assert_eq!(
            key_bytes(text).expect("the key is valid"),
            expected,
            "{text}"
        );
    }
}

#[test]
fn a_broken_key_is_reported() {
    for text in ["ctrl+", "ctrl+shift", r"\x0", r"\q"] {
        let error = key_bytes(text).expect_err("the key is broken");
        assert!(matches!(error, ScriptError::InvalidKey { .. }), "{text}");
    }
}
