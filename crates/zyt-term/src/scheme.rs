//! Whether the page is dark or light, as a program that asked is told.
//!
//! A program that picks its colours for a dark or a light page asks to be told
//! when that changes: it sets private mode 2031, and the terminal then says
//! `CSI ? 997 ; 1 n` for dark and `CSI ? 997 ; 2 n` for light. The same report
//! answers `CSI ? 996 n`, the question of the scheme as it stands.
//!
//! The backend knows neither, so the sniffer picks them up
//! (`crate::osc`), and the state is kept here. Which scheme the page has is
//! the caller's answer, the way its colours are: the theme is the interface.

use crate::Rgb;

/// Private mode a program sets to be told when the scheme changes.
pub(crate) const REPORTS_MODE: u16 = 2031;

/// Private status report a program asks for the scheme as it stands.
pub(crate) const QUERY: u16 = 996;

/// Report of a dark page.
const DARK_REPORT: &[u8] = b"\x1b[?997;1n";

/// Report of a light page.
const LIGHT_REPORT: &[u8] = b"\x1b[?997;2n";

/// Weights of red, green and blue in the luma of a colour, of a thousand.
const LUMA_WEIGHTS: (u32, u32, u32) = (299, 587, 114);

/// Luma at the middle between black and white, on the scale of `LUMA_WEIGHTS`.
const MIDDLE_LUMA: u32 = 255 * 1000 / 2;

/// Whether the page is dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    /// Light text on a dark page.
    Dark,
    /// Dark text on a light page.
    Light,
}

impl ColorScheme {
    /// The scheme of a page painted in `background`: light when its luma is
    /// past the middle between black and white.
    pub fn of(background: Rgb) -> Self {
        let (red, green, blue) = LUMA_WEIGHTS;
        let luma = red * u32::from(background.r)
            + green * u32::from(background.g)
            + blue * u32::from(background.b);
        if luma > MIDDLE_LUMA {
            Self::Light
        } else {
            Self::Dark
        }
    }

    fn report(self) -> &'static [u8] {
        match self {
            Self::Dark => DARK_REPORT,
            Self::Light => LIGHT_REPORT,
        }
    }
}

/// The scheme the caller said, and whether a program asked to hear of it.
#[derive(Debug, Default)]
pub(crate) struct SchemeReports {
    scheme: Option<ColorScheme>,
    wanted: bool,
}

impl SchemeReports {
    /// Whether mode 2031 is set: a program asked to hear of the scheme.
    pub(crate) fn wanted(&self) -> bool {
        self.wanted
    }

    /// Sets or resets mode 2031.
    pub(crate) fn set_wanted(&mut self, wanted: bool) {
        self.wanted = wanted;
    }

    /// The answer to `CSI ? 996 n`, while the caller has said a scheme.
    pub(crate) fn answer(&self) -> Option<&'static [u8]> {
        self.scheme.map(ColorScheme::report)
    }

    /// Takes the scheme the caller says, and gives the report a program that
    /// asked is sent.
    ///
    /// The first scheme is no change: a program that set the mode asks for the
    /// scheme as it stands, and is told of changes from then on.
    pub(crate) fn change(&mut self, scheme: ColorScheme) -> Option<&'static [u8]> {
        let before = self.scheme.replace(scheme);
        let changed = before.is_some_and(|before| before != scheme);
        (changed && self.wanted).then(|| scheme.report())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_dark_or_light_by_its_luma() {
        let of = |r, g, b| ColorScheme::of(Rgb { r, g, b });
        assert_eq!(of(0, 0, 0), ColorScheme::Dark);
        assert_eq!(of(0x1b, 0x1b, 0x1b), ColorScheme::Dark);
        assert_eq!(of(0x80, 0x80, 0x80), ColorScheme::Light);
        assert_eq!(of(0xfd, 0xf6, 0xe3), ColorScheme::Light);
        assert_eq!(of(0, 0, 0xff), ColorScheme::Dark);
        assert_eq!(of(0xff, 0xff, 0xff), ColorScheme::Light);
    }
}
