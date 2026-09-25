//! One command of the protocol, as the shell that carries it.
//!
//! Every command is a file under `scripts/`, built into the program. A script
//! is written there the way it reads best — several lines, indented, with a
//! comment saying why — and folded into the one line the wire takes before it
//! is sent, because the shell of a device must not be left waiting for the
//! rest of a construct.
//!
//! The holes of a script are written `{name}` and are filled by the caller.
//! The reply markers are not holes: a script says `\echo '##''# 200'` itself,
//! in the two pieces a console cannot echo back as a reply. See [`crate::wire`].

/// The shell of one command, and the filling of its holes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    body: &'static str,
}

impl Command {
    /// The command a script file holds.
    pub const fn new(body: &'static str) -> Self {
        Self { body }
    }

    /// The script as it was written, before anything was done to it.
    pub fn body(self) -> &'static str {
        self.body
    }

    /// Fills the holes and folds the script into the one line the wire takes.
    pub fn render(self, values: &[(&str, &str)]) -> String {
        fill(&one_line(self.body), values)
    }
}

/// Everything but the comments and the blank lines, joined by a space.
fn one_line(template: &str) -> String {
    template
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Puts a value in every hole, in one pass.
///
/// What is put in is not looked at again, so a path that carries `{size}` in
/// its own name fills nothing. A hole nothing answers to is left as it stands,
/// which shows up in a test rather than in a quietly wrong command.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('}') else {
            break;
        };
        match value_of(&rest[1..close], values) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[..=close]),
        }
        rest = &rest[close + 1..];
    }

    out.push_str(rest);
    out
}

/// What answers to the name of a hole.
fn value_of<'a>(name: &str, values: &'a [(&str, &str)]) -> Option<&'a str> {
    values
        .iter()
        .find(|(hole, _)| *hole == name)
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_is_folded_into_one_line() {
        let folded = one_line("# why\n\n  \\echo one;\n  \\echo two\n");
        assert_eq!(folded, "\\echo one; \\echo two");
    }

    #[test]
    fn a_marker_is_written_by_the_script_and_not_filled_in() {
        let command = Command::new("\\echo '##''# 200'");
        assert_eq!(command.render(&[]), "\\echo '##''# 200'");
    }

    #[test]
    fn what_was_put_in_is_not_looked_at_again() {
        let command = Command::new("a {path} b");
        let filled = command.render(&[("path", "'{size}'"), ("size", "9")]);
        assert_eq!(filled, "a '{size}' b");
    }

    #[test]
    fn a_hole_nothing_answers_to_stays_as_it_is() {
        assert_eq!(Command::new("a {nobody} b").render(&[]), "a {nobody} b");
        assert_eq!(Command::new("a { b").render(&[]), "a { b");
    }

    #[test]
    fn every_hole_is_filled_not_only_the_first() {
        let command = Command::new("{path} {path} {size}");
        let filled = command.render(&[("path", "p"), ("size", "9")]);
        assert_eq!(filled, "p p 9");
    }
}
