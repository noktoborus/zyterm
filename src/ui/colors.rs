//! The colors of the window, worked out from the theme of the terminal: the
//! window is a little darker than the terminal it holds, and what it marks it
//! marks in the colors of the same palette (`docs/terminal.md`, *Themes*).
//!
//! A role whose color would not read on the window is left out, and the
//! toolkit keeps its own for it.

use egui::Color32;
use palette::{Clamp, FromColor, LinSrgb, Oklab, Srgb};
use zyt_term_egui::TerminalTheme;

/// How much darker the window is than the terminal, in the lightness of Oklab.
const DARKER: f32 = 0.05;

/// How far a control stands from the window, at rest, under the pointer and
/// pressed: lighter on a dark window, darker on a light one.
const WIDGET_STEPS: [f32; 3] = [0.06, 0.10, 0.14];

/// How far a stripe, and a line between parts, stand from the window.
const FAINT_STEP: f32 = 0.03;
const STROKE_STEP: f32 = 0.12;

/// The least contrast of text on the window, as WCAG counts it.
const TEXT_CONTRAST: f32 = 4.5;

/// The least contrast of a color that marks something: a warning, a link,
/// faint text.
const MARK_CONTRAST: f32 = 3.0;

/// The colors of the window, one for each role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    /// The window is dark: the toolkit starts from its dark colors.
    pub dark: bool,
    /// The status bar, the menus, the plates, the pages.
    pub panel: Color32,
    /// A field text is typed into: the background of the terminal.
    pub field: Color32,
    /// A stripe of a table, the background of code.
    pub faint: Color32,
    /// A control at rest, under the pointer, pressed.
    pub widgets: [Color32; 3],
    /// A line between parts, the edge of a window.
    pub stroke: Color32,
    /// Text.
    pub text: Option<Color32>,
    /// Text of less weight.
    pub weak_text: Option<Color32>,
    /// Behind selected text.
    pub selection: Option<Color32>,
    /// A link: the blue of the palette.
    pub link: Option<Color32>,
    /// A warning: the yellow of the palette.
    pub warn: Option<Color32>,
    /// An error: the red of the palette.
    pub error: Option<Color32>,
    /// What holds, what did its work: the green of the palette.
    pub ok: Option<Color32>,
}

impl Colors {
    /// The colors of the window for a theme of the terminal.
    pub fn from_theme(theme: &TerminalTheme) -> Self {
        let dark = crate::themes::is_dark(Some(theme.background));
        let panel = lighter(theme.background, -DARKER);
        let away = if dark { 1.0 } else { -1.0 };
        let readable =
            |color: Color32, least: f32| (contrast(color, panel) >= least).then_some(color);
        let marking = |normal: usize| {
            let (plain, bright) = (theme.palette[normal], theme.palette[normal + 8]);
            let best = if contrast(plain, panel) >= contrast(bright, panel) {
                plain
            } else {
                bright
            };
            readable(best, MARK_CONTRAST)
        };
        Self {
            dark,
            panel,
            field: theme.background,
            faint: lighter(panel, away * FAINT_STEP),
            widgets: WIDGET_STEPS.map(|step| lighter(panel, away * step)),
            stroke: lighter(panel, away * STROKE_STEP),
            text: readable(theme.foreground, TEXT_CONTRAST),
            weak_text: readable(theme.dim_foreground, MARK_CONTRAST),
            selection: (contrast(theme.selection, theme.foreground) >= MARK_CONTRAST)
                .then_some(theme.selection),
            link: marking(4),
            warn: marking(3),
            error: marking(1),
            ok: marking(2),
        }
    }
}

/// The color with its lightness in Oklab moved by `by`, its hue and its
/// chroma kept.
fn lighter(color: Color32, by: f32) -> Color32 {
    let mut lab = Oklab::from_color(srgb(color).into_linear::<f32>());
    lab.l = (lab.l + by).clamp(0.0, 1.0);
    let back: Srgb<u8> = Srgb::from_linear(LinSrgb::from_color(lab).clamp());
    Color32::from_rgb(back.red, back.green, back.blue)
}

/// The contrast of two colors as WCAG counts it, from 1 to 21.
fn contrast(one: Color32, other: Color32) -> f32 {
    let luminance = |color: Color32| {
        let linear: LinSrgb<f32> = srgb(color).into_linear();
        0.2126 * linear.red + 0.7152 * linear.green + 0.0722 * linear.blue
    };
    let (one, other) = (luminance(one), luminance(other));
    (one.max(other) + 0.05) / (one.min(other) + 0.05)
}

fn srgb(color: Color32) -> Srgb<u8> {
    Srgb::new(color.r(), color.g(), color.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lightness(color: Color32) -> f32 {
        Oklab::from_color(srgb(color).into_linear::<f32>()).l
    }

    #[test]
    fn the_window_is_darker_than_the_terminal_on_every_theme() {
        for theme in [TerminalTheme::dark(), TerminalTheme::light()] {
            let colors = Colors::from_theme(&theme);
            assert!(lightness(colors.panel) < lightness(theme.background));
            assert_eq!(colors.field, theme.background);
        }
    }

    #[test]
    fn the_hue_of_the_background_is_kept() {
        let theme = TerminalTheme {
            background: Color32::from_rgb(0x00, 0x2b, 0x36),
            ..TerminalTheme::dark()
        };
        let panel = Colors::from_theme(&theme).panel;
        assert!(panel.b() > panel.r() && panel.g() > panel.r(), "{panel:?}");
    }

    #[test]
    fn controls_stand_away_from_the_window_toward_the_middle() {
        let dark = Colors::from_theme(&TerminalTheme::dark());
        assert!(dark.dark);
        assert!(lightness(dark.widgets[0]) > lightness(dark.panel));
        let light = Colors::from_theme(&TerminalTheme::light());
        assert!(!light.dark);
        assert!(lightness(light.widgets[0]) < lightness(light.panel));
    }

    #[test]
    fn a_color_that_would_not_read_is_left_to_the_toolkit() {
        let theme = TerminalTheme {
            background: Color32::from_rgb(0x30, 0x30, 0x30),
            foreground: Color32::from_rgb(0x38, 0x38, 0x38),
            selection: Color32::from_rgb(0x38, 0x38, 0x38),
            ..TerminalTheme::dark()
        };
        let colors = Colors::from_theme(&theme);
        assert_eq!(colors.text, None);
        assert_eq!(colors.selection, None);
    }

    #[test]
    fn a_mark_takes_the_plain_or_bright_entry_that_reads_better() {
        let mut theme = TerminalTheme::light();
        theme.palette[3] = Color32::from_rgb(0xff, 0xff, 0x60);
        theme.palette[11] = Color32::from_rgb(0x80, 0x60, 0x00);
        assert_eq!(Colors::from_theme(&theme).warn, Some(theme.palette[11]));
    }
}
