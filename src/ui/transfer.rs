//! The plate of a run: a script holding the line, in the middle of the
//! terminal, saying where it stands, with the way out of it.
//!
//! While such a script runs the keyboard reaches nothing and the device is not
//! drawn, so the window is doing one thing, and the plate is that thing. It
//! stands from the start of the run to the end of its line and is the one
//! place the run is stopped from over the terminal; a dialog of the script and
//! a menu stand in its place while they are open, because the run waits on
//! them.

use crate::app::App;
use crate::session::RunPhase;
use rust_i18n::t;
use zyt_script::{Direction, Stage, StageKind};
use zyt_term::ProgressState;

/// How wide the plate is: one width, whatever the stage says, so the plate
/// does not move while the stage changes from one word to a long name.
const WIDTH: f32 = 360.0;

/// How often the plate is drawn again while it shows a speed.
const SPEED_TICK: std::time::Duration = std::time::Duration::from_millis(500);

/// What stands in a row with nothing to say, so the plate keeps its rows.
const BLANK: &str = " ";

/// Draws the plate over `area`, the rectangle of the terminal.
pub fn plate(app: &mut App, ui: &mut egui::Ui, area: egui::Rect) {
    let session = &app.session;
    if !session.run_holds_line() || app.ui.form.is_some() || app.menu.is_open() {
        return;
    }
    let Some(phase) = session.run_phase() else {
        return;
    };

    let name = session.script_name().unwrap_or_default();
    let header = match session.run_direction() {
        Some(Direction::Send) => format!("{name} · {}", t!("transfer.sending")),
        Some(Direction::Receive) => format!("{name} · {}", t!("transfer.receiving")),
        None => name.to_string(),
    };
    let stage = stage_line(phase, session.run_stage());
    let progress = session.run_progress();
    let speed = session.run_speed();
    let figures = figures(session.run_bytes(), progress, speed);
    // The speed falls by itself once the bytes stop, and a run that says
    // nothing wakes no frame.
    if speed.is_some() {
        ui.ctx().request_repaint_after(SPEED_TICK);
    }

    let clicked = egui::Area::new(ui.id().with("run_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(area.center())
        .pivot(egui::Align2::CENTER_CENTER)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    let (_, clicked) = egui::Sides::new().shrink_left().truncate().show(
                        ui,
                        |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&header).strong()).truncate(),
                            );
                        },
                        |ui| {
                            ui.add_enabled(
                                phase != RunPhase::Stopping,
                                egui::Button::new(t!("transfer.cancel")),
                            )
                            .on_hover_text(t!("status.cancel_transfer_hint"))
                            .clicked()
                        },
                    );
                    ui.add(egui::Label::new(stage).truncate());
                    // A run that says nothing of how far it is has no bar: an
                    // empty one would read as a share that never moves.
                    if progress.is_some() {
                        bar(ui, progress, None, false);
                    }
                    ui.add(egui::Label::new(egui::RichText::new(figures).weak()).truncate());
                    clicked
                })
                .inner
        })
        .inner;

    if clicked {
        let outcome = app.session.cancel_transfer();
        app.report(outcome);
    }
}

/// The bar of the share: empty while there is none, in the colour of an error
/// or of a pause when the script says so. `width` sets it, the whole width of
/// what holds it otherwise; `numbered` writes the share in it.
pub(super) fn bar(
    ui: &mut egui::Ui,
    progress: Option<ProgressState>,
    width: Option<f32>,
    numbered: bool,
) {
    let visuals = ui.visuals();
    let (share, color, hint) = match progress {
        Some(ProgressState::Set(share)) => (
            Some(share),
            visuals.selection.bg_fill,
            t!("status.progress"),
        ),
        Some(ProgressState::Error(share)) => (
            Some(share),
            visuals.error_fg_color,
            t!("status.progress_error"),
        ),
        Some(ProgressState::Paused(share)) => (
            Some(share),
            visuals.warn_fg_color,
            t!("status.progress_paused"),
        ),
        Some(ProgressState::Indeterminate | ProgressState::Removed) | None => (
            None,
            visuals.selection.bg_fill,
            t!("status.progress_unknown"),
        ),
    };
    let fill = share.map_or(0.0, |share| f32::from(share) / 100.0);
    let mut bar = egui::ProgressBar::new(fill).fill(color);
    if let Some(width) = width {
        bar = bar.desired_width(width);
    }
    if let Some(share) = share.filter(|_| numbered) {
        bar = bar.text(format!("{share}%"));
    }
    ui.add(bar).on_hover_text(hint);
}

/// What the row of the stage says: the script's stage while it runs, and what
/// the run is doing once it is past the script.
pub(super) fn stage_line(phase: RunPhase, stage: Option<&Stage>) -> String {
    match phase {
        RunPhase::Stopping => t!("transfer.stage.stopping").to_string(),
        RunPhase::Draining => t!("transfer.stage.draining").to_string(),
        RunPhase::Running => stage.map_or_else(|| t!("transfer.stage.running").to_string(), worded),
    }
}

/// A stage in the words of this window: what is going on, then the file, which
/// of how many it is, and what the script added, each after a dot.
fn worded(stage: &Stage) -> String {
    let kind = match stage.kind {
        StageKind::Preparing => Some(t!("transfer.stage.preparing")),
        StageKind::Telling => Some(t!("transfer.stage.telling")),
        StageKind::Listing => Some(t!("transfer.stage.listing")),
        StageKind::Negotiating => Some(t!("transfer.stage.negotiating")),
        StageKind::Sending => Some(t!("transfer.stage.sending")),
        StageKind::Receiving => Some(t!("transfer.stage.receiving")),
        StageKind::Retrying => Some(t!(
            "transfer.stage.retrying",
            count = stage.count.unwrap_or(1)
        )),
        StageKind::Finishing => Some(t!("transfer.stage.finishing")),
        StageKind::Other => None,
    };

    let mut parts: Vec<String> = kind.into_iter().map(|kind| kind.to_string()).collect();
    parts.extend(stage.name.clone());
    if let (Some(file), Some(files)) = (stage.file, stage.files) {
        parts.push(t!("transfer.stage.of_files", file = file, files = files).to_string());
    }
    parts.extend(stage.text.clone());
    if parts.is_empty() {
        return t!("transfer.stage.running").to_string();
    }
    parts.join(" · ")
}

/// The row of figures: how much went, of how much, the share and the speed,
/// each that is known, after a dot.
fn figures(
    bytes: Option<(u64, Option<u64>)>,
    progress: Option<ProgressState>,
    speed: Option<f32>,
) -> String {
    let share = match progress {
        Some(
            ProgressState::Set(share) | ProgressState::Error(share) | ProgressState::Paused(share),
        ) => Some(share),
        _ => None,
    };
    let size = crate::format::size;
    let mut parts = Vec::new();
    match bytes {
        Some((done, Some(whole))) => {
            parts.push(t!("transfer.bytes_of", done = size(done), whole = size(whole)).to_string())
        }
        Some((done, None)) => parts.push(size(done)),
        None => {}
    }
    parts.extend(share.map(|share| format!("{share}%")));
    parts.extend(speed.map(crate::format::rate));
    if parts.is_empty() {
        return BLANK.to_string();
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(kind: StageKind) -> Stage {
        Stage {
            kind,
            name: None,
            file: None,
            files: None,
            count: None,
            text: None,
        }
    }

    #[test]
    fn a_stage_is_worded_with_its_file_and_its_place_among_the_files() {
        rust_i18n::set_locale("en");
        let mut sending = stage(StageKind::Sending);
        sending.name = Some("foo.bin".to_string());
        sending.file = Some(2);
        sending.files = Some(5);
        assert_eq!(worded(&sending), "Sending · foo.bin · 2 of 5");

        let mut other = stage(StageKind::Other);
        other.text = Some("warming up".to_string());
        assert_eq!(worded(&other), "warming up", "a text alone is said alone");

        assert_eq!(
            worded(&stage(StageKind::Other)),
            "Running",
            "a stage that says nothing reads as running"
        );
    }

    #[test]
    fn past_the_script_the_run_says_what_it_is_doing_whatever_the_stage() {
        rust_i18n::set_locale("en");
        let negotiating = stage(StageKind::Negotiating);
        assert_eq!(
            stage_line(RunPhase::Running, Some(&negotiating)),
            "Waiting for the device to agree"
        );
        assert_eq!(stage_line(RunPhase::Running, None), "Running");
        assert_eq!(
            stage_line(RunPhase::Stopping, Some(&negotiating)),
            "Stopping…"
        );
        assert_eq!(
            stage_line(RunPhase::Draining, None),
            "Sending the last bytes"
        );
    }

    #[test]
    fn the_figures_say_what_is_known_and_keep_their_row_otherwise() {
        rust_i18n::set_locale("en");
        let half = Some(ProgressState::Set(50));
        let size = crate::format::size;
        assert_eq!(
            figures(Some((512, Some(1024))), half, Some(11.25)),
            format!(
                "{} of {} · 50% · {}",
                size(512),
                size(1024),
                crate::format::rate(11.25)
            )
        );
        assert_eq!(
            figures(Some((512, None)), Some(ProgressState::Indeterminate), None),
            size(512)
        );
        assert_eq!(figures(None, half, None), "50%");
        assert_eq!(figures(None, None, None), BLANK);
    }
}
