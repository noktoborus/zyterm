//! Icons of the interface, one code point each.
//!
//! egui draws with the fonts it ships with, and a code point none of them
//! carries becomes a box on the screen. Three of those fonts answer the
//! proportional family the interface is drawn in — Ubuntu-Light,
//! NotoEmoji-Regular and emoji-icon-font — so the set of icons to choose from is
//! small and worth keeping in one place. The test at the bottom walks it: an
//! icon no font carries fails the suite instead of reaching a window.
//!
//! Which code points there are to choose from is what `experiments/glyphs`
//! writes down — `cargo run -p glyphs` — so an icon is taken from that list
//! rather than guessed at and tried.
//!
//! The test asks the charmaps of the family, the way that tool does, and not
//! `Fonts::has_glyph`: that one answers no for every code point whose first
//! face in the chain is also the face the toolkit takes its replacement glyph
//! from, which in the proportional family is NotoEmoji-Regular. A magnifier and
//! a check mark are both in it, and both were called missing.

/// Settings, the gear.
pub const SETTINGS: &str = "\u{2699}";
/// One more entry of a list, the cross.
pub const ADD: &str = "\u{271a}";
/// Throwing an entry away, the waste basket.
pub const REMOVE: &str = "\u{1f5d1}";
/// Copying, the sheets.
pub const COPY: &str = "\u{1f5d0}";
/// Starting a console again when it ends, the clockwise arrow.
pub const RESTART: &str = "\u{21bb}";
/// Reading the list again, the turning arrow.
pub const REFRESH: &str = "\u{27f3}";
/// Closing a panel or stopping what runs, the cross in a box.
pub const CLOSE: &str = "\u{1f5d9}";
/// The way back out of the settings, the left triangle.
pub const BACK: &str = "\u{23f4}";
/// A session whose output may be acted on, the shield.
pub const TRUSTED: &str = "\u{1f6e1}";
/// A session whose output may not, the skull.
pub const UNTRUSTED: &str = "\u{2620}";

/// The color those two wear, wherever one of them stands.
///
/// The shield is the green of a connection that holds and the skull is the
/// color the toolkit warns in, so the two say which one they are before the
/// shape of them is read — and they say it the same way in the status bar and
/// in the settings, which is why the color is here and not beside each of them.
pub fn trust_color(ui: &egui::Ui, trusted: bool) -> egui::Color32 {
    if trusted {
        egui::Color32::from_rgb(0x5c, 0xb8, 0x5c)
    } else {
        ui.visuals().warn_fg_color
    }
}
/// What one value becomes, the right triangle.
pub const TO: &str = "\u{23f5}";
/// Stepping towards the beginning of the scrollback, the up triangle.
pub const UP: &str = "\u{23f6}";
/// Stepping towards the newest line, the down triangle.
pub const DOWN: &str = "\u{23f7}";
/// Reading the query as the text it is, the capital T.
pub const LITERAL: &str = "\u{ff34}";
/// Reading the query loosely, the almost equal sign.
pub const FUZZY: &str = "\u{2248}";
/// Telling an upper case letter from a lower case one, the capital A.
pub const CASE: &str = "\u{ff21}";
/// Finding a word and not a part of one, the capital W.
pub const WORD: &str = "\u{ff37}";
/// Reading the query as a pattern, the asterisk.
pub const REGEX: &str = "\u{2731}";
/// Marking every match and not only one, the shaded block.
pub const HIGHLIGHT: &str = "\u{2593}";
/// Choosing a directory, the folder.
pub const FOLDER: &str = "\u{1f5c1}";
/// The entry of a menu that names what is in use, the bullet.
pub const CURRENT: &str = "\u{2022}";
/// What was cut off a name too long to show, the ellipsis.
pub const ELLIPSIS: &str = "\u{2026}";
/// The commands a shell marked, the arrow turning back on itself.
pub const HISTORY: &str = "\u{21ba}";
/// The stream of the session held by a program of this side, the chains.
pub const CAPTURED: &str = "\u{26d3}";
/// The mouse asked for by the program of the session, the mouse.
pub const MOUSE: &str = "\u{1f5b1}";
/// Looking through what the screen and the scrollback hold, the magnifier.
pub const SEARCH: &str = "\u{1f50d}";
/// Everything this window has running, the clipboard.
pub const TASKS: &str = "\u{1f4cb}";
/// What a program beside the line wrote down, the page.
pub const LOG: &str = "\u{1f4c4}";
/// Opening a value for writing, the pencil.
pub const EDIT: &str = "\u{270f}";
/// Something that will not work as it stands, the warning triangle.
pub const WARNING: &str = "\u{26a0}";
/// A signal this side drives, the right pointing triangle.
///
/// The same code point as [`TO`], because it is the same shape doing the same
/// job in another place: a value on the left becomes a value on the right, and
/// a signal on the left leaves towards the device on the right. A plain arrow
/// would say it better, and `U+2192` is in no font of the proportional family —
/// `cargo run -p glyphs` says only Hack carries it, and Hack answers Monospace.
pub const OUTGOING: &str = "\u{23f5}";
/// A signal the peer drives, the left pointing triangle.
pub const INCOMING: &str = "\u{23f4}";

/// Every icon, for the test that the fonts carry them all.
#[cfg(test)]
pub const ALL: &[&str] = &[
    SETTINGS, ADD, REMOVE, COPY, RESTART, REFRESH, CLOSE, BACK, TRUSTED, UNTRUSTED, FOLDER, TO, UP,
    DOWN, CASE, WORD, REGEX, HIGHLIGHT, ELLIPSIS, HISTORY, CAPTURED, MOUSE, SEARCH, TASKS, LOG,
    EDIT, WARNING, OUTGOING, INCOMING,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The icons of a list that no face of the family has a glyph for.
    ///
    /// The charmaps of the family are asked, one code point at a time, which is
    /// the question `experiments/glyphs` answers as well.
    fn missing<'a>(family: egui::FontFamily, icons: &'a [&'a str]) -> Vec<&'a str> {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        context.fonts_mut(move |fonts| {
            let carried = fonts.fonts.font(&family).characters().clone();
            icons
                .iter()
                .copied()
                .filter(|icon| icon.chars().any(|point| !carried.contains_key(&point)))
                .collect()
        })
    }

    #[test]
    fn every_icon_is_in_the_fonts_the_toolkit_ships_with() {
        let missing = missing(egui::FontFamily::Proportional, ALL);

        assert!(
            missing.is_empty(),
            "no font of the toolkit carries these: {missing:?}"
        );
    }

    /// A code point nothing carries is what the test is for, so it says so.
    #[test]
    fn a_code_point_no_font_carries_is_named() {
        let missing = missing(egui::FontFamily::Proportional, &["\u{e000}"]);

        assert_eq!(missing, ["\u{e000}"]);
    }
}
