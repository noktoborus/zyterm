//! The identifiers of tmux: `$` a session, `@` a window, `%` a pane.

use std::fmt;

macro_rules! id {
    ($(#[$doc:meta])* $name:ident, $sigil:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);

        impl $name {
            /// The sign this kind of identifier is written with.
            pub const SIGIL: u8 = $sigil;

            /// Reads `$1`, `@1` or `%1` as tmux writes it; a number past `u32`
            /// is none.
            pub fn parse(word: &[u8]) -> Option<Self> {
                let (&sigil, digits) = word.split_first()?;
                (sigil == Self::SIGIL).then_some(())?;
                number(digits).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(out, "{}{}", char::from(Self::SIGIL), self.0)
            }
        }
    };
}

id!(
    /// A session, `$1`.
    SessionId,
    b'$'
);
id!(
    /// A window, `@1`.
    WindowId,
    b'@'
);
id!(
    /// A pane, `%1`.
    PaneId,
    b'%'
);

/// Reads decimal digits and nothing else into a `u32`.
pub(crate) fn number(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_read_and_write() {
        assert_eq!(PaneId::parse(b"%12"), Some(PaneId(12)));
        assert_eq!(WindowId::parse(b"@0"), Some(WindowId(0)));
        assert_eq!(SessionId::parse(b"$4294967295"), Some(SessionId(u32::MAX)));
        assert_eq!(SessionId::parse(b"$4294967296"), None);
        assert_eq!(PaneId::parse(b"@1"), None);
        assert_eq!(PaneId::parse(b"%"), None);
        assert_eq!(PaneId::parse(b"%1x"), None);
        assert_eq!(PaneId::parse(b"%+1"), None);
        assert_eq!(PaneId(7).to_string(), "%7");
    }
}
