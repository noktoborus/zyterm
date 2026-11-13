//! The colors of the window handed to the toolkit: [`Colors`] laid over its
//! own dark or light colors, a role left out keeping the toolkit's.

use egui::{Color32, Visuals};

use super::colors::Colors;

/// What holds, what did its work, while the theme gives no green that reads.
const OK: Color32 = Color32::from_rgb(0x5c, 0xb8, 0x5c);

/// Where the green of the window is kept: egui has no role for it.
fn ok_id() -> egui::Id {
    egui::Id::unique("zyterm_ok_color")
}

/// Gives the toolkit the colors of the window, for both of its themes: the
/// toolkit turns between them with the desktop, and the colors of the window
/// are those of the terminal whichever the desktop is.
pub fn apply(context: &egui::Context, colors: &Colors) {
    let mut visuals = if colors.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = colors.panel;
    visuals.window_fill = colors.panel;
    visuals.extreme_bg_color = colors.field;
    visuals.text_edit_bg_color = Some(colors.field);
    visuals.faint_bg_color = colors.faint;
    visuals.code_bg_color = colors.faint;
    visuals.window_stroke.color = colors.stroke;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = colors.panel;
    widgets.noninteractive.weak_bg_fill = colors.panel;
    widgets.noninteractive.bg_stroke.color = colors.stroke;
    let [rest, hovered, pressed] = colors.widgets;
    for (widget, fill) in [
        (&mut widgets.inactive, rest),
        (&mut widgets.hovered, hovered),
        (&mut widgets.active, pressed),
        (&mut widgets.open, rest),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
    }
    if let Some(text) = colors.text {
        for widget in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            widget.fg_stroke.color = text;
        }
    }

    visuals.weak_text_color = colors.weak_text;
    if let Some(selection) = colors.selection {
        visuals.selection.bg_fill = selection;
    }
    if let Some(link) = colors.link {
        visuals.hyperlink_color = link;
    }
    if let Some(warn) = colors.warn {
        visuals.warn_fg_color = warn;
    }
    if let Some(error) = colors.error {
        visuals.error_fg_color = error;
    }
    context.set_visuals_of(egui::Theme::Light, visuals.clone());
    context.set_visuals_of(egui::Theme::Dark, visuals);
    context.data_mut(|data| data.insert_temp(ok_id(), colors.ok.unwrap_or(OK)));
}

/// The green of what holds or did its work: a connection, a line up, a job
/// done, a trusted source.
pub fn ok_color(ui: &egui::Ui) -> Color32 {
    ui.ctx().data(|data| data.get_temp(ok_id())).unwrap_or(OK)
}
