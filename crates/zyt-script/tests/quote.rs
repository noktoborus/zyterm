//! Quoting a path for the shell that is going to read it.

use zyt_script::{quote_for_shell, quote_posix};

#[test]
fn a_path_is_quoted_as_one_word() {
    let quoted = quote_for_shell("/tmp/two words/it's here");

    if cfg!(windows) {
        assert_eq!(quoted, "\"/tmp/two words/it's here\"");
    } else {
        assert_eq!(quoted, r"'/tmp/two words/it'\''s here'");
    }
}

#[test]
fn the_device_is_quoted_for_a_posix_shell_whatever_this_machine_is() {
    assert_eq!(
        quote_posix("/tmp/two words/it's here"),
        r"'/tmp/two words/it'\''s here'"
    );
}

#[test]
fn a_quoted_path_is_one_word_to_a_shell() {
    let path = "/tmp/two words";
    let line = format!("printf '%s' {}", quote_posix(path));
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&line)
        .output()
        .expect("the shell runs");
    assert_eq!(String::from_utf8_lossy(&output.stdout), path);
}
