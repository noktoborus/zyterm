//! The wheel turned with a key held, counted in notches.
//!
//! The toolkit turns the wheel into a smooth movement of points, and with
//! `Shift` or `Ctrl` held into a sideways movement or a zoom. A step that a
//! notch is worth — a screen, a size of the font — is read from the wheel
//! events themselves: a wheel of notches gives one each, and a touchpad that
//! moves by points gives one every [`NOTCH`] points, the rest kept for the
//! next frame.

use egui::{Event, Modifiers, MouseWheelUnit};

/// Points of a touchpad that are worth a notch: what the toolkit makes of one
/// line of a wheel.
const NOTCH: f32 = 40.0;

/// Notches the wheel turned this frame with `held` true of the keys, positive
/// away from the user. `id` keeps what is short of a notch.
///
/// A wheel turned with `Shift` held may come sideways — some systems turn it
/// so — and is counted the same.
pub fn notches(ui: &egui::Ui, id: egui::Id, held: impl Fn(&Modifiers) -> bool) -> i32 {
    let mut turned = ui.data(|data| data.get_temp::<f32>(id).unwrap_or(0.0));
    ui.input(|input| {
        for event in &input.events {
            if let Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
                && held(modifiers)
            {
                turned += match unit {
                    MouseWheelUnit::Point => (delta.x + delta.y) / NOTCH,
                    MouseWheelUnit::Line | MouseWheelUnit::Page => delta.x + delta.y,
                };
            }
        }
    });
    ui.data_mut(|data| data.insert_temp(id, turned.fract()));
    turned.trunc() as i32
}
