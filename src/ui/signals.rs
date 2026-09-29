//! The plate of the signals, over the terminal.
//!
//! A row of letters says what the lines are doing now, which is not what
//! somebody watching a line came to find out. A board pulled `DTR` and went
//! quiet, `CTS` fell for half a second and the data stopped with it, a device
//! raised `DCD` and dropped it again: all of it happens between two glances, and
//! the row looks the same afterwards as it did before. This plate is the order
//! those things happened in, so the one question a serial line always raises —
//! did it go quiet by itself, or did a signal stop it — is answered by looking
//! rather than by guessing.
//!
//! It rises while the pointer rests on one of those letters and goes when the
//! pointer leaves. Nothing pins it: it covers the newest rows of the output, and
//! a thing that covers what is being read has to be the thing the hand is
//! already doing.
//!
//! The two groups are the two directions. What this side drives stands above
//! what the peer drives, each with the data of its own direction at the head of
//! it, because the handshake is what leads to the bytes.

use crate::app::App;
use crate::ui::icons;
use std::time::Duration;
use zyt_serial::{LineSample, Signal};

/// One row of the plate.
struct Track {
    /// The three letters the row is called by, as the status bar calls them.
    name: &'static str,
    /// What it draws.
    signal: Signal,
}

/// The rows, in the order they stand.
///
/// What this side drives first, what the peer drives after it, and the data of
/// each direction at the head of its own group. The order is here and not in
/// `zyt-serial` because it is a question about reading a picture: the crate
/// knows which signals there are and which of them this side drives, and a
/// window is what knows the order they are read in.
const TRACKS: [Track; 8] = [
    Track {
        name: "TX",
        signal: Signal::Sent,
    },
    Track {
        name: "BRK",
        signal: Signal::Break,
    },
    Track {
        name: "RTS",
        signal: Signal::Rts,
    },
    Track {
        name: "DTR",
        signal: Signal::Dtr,
    },
    Track {
        name: "RX",
        signal: Signal::Received,
    },
    Track {
        name: "CTS",
        signal: Signal::Cts,
    },
    Track {
        name: "DSR",
        signal: Signal::Dsr,
    },
    Track {
        name: "DCD",
        signal: Signal::Carrier,
    },
];

/// How wide one sample stands.
///
/// Three points is the narrowest a bar can be and still be seen as a bar at the
/// scale a window is drawn at; narrower, and a signal that stood up for one
/// poll is a hair nobody notices.
const BAR_WIDTH: f32 = 3.0;

/// How far the plate stands from the bar it rises over.
const PLATE_GAP: f32 = 6.0;

/// Room between the name of a row, its arrow and the track.
const LABEL_GAP: f32 = 6.0;

/// How much of the colour of the text the empty part of a track keeps.
///
/// A track where the signal never stood would otherwise be nothing at all, and a
/// row of nothing reads as a row that is missing rather than as a signal that
/// never came.
const EMPTY_SHARE: f32 = 0.12;

/// Draws the plate while the pointer rests on one of the line indicators.
///
/// It is called from the status bar, which is where the pointer that raises it
/// is, and it draws into an area of its own — so it stands over the terminal
/// whatever the bar around it is doing.
pub fn plate(app: &mut App, ui: &mut egui::Ui) {
    if !app.ui.signals_hovered {
        return;
    }

    // The plate is a picture of a clock that runs whether a frame is drawn or
    // not, so it asks for one per sample while it stands and for nothing once
    // it is gone. Nowhere else asks.
    let interval = app.session.lines_interval();
    ui.ctx().request_repaint_after(interval);

    let screen = ui.ctx().content_rect();
    let above = egui::pos2(screen.left(), ui.max_rect().top() - PLATE_GAP);
    let frame = egui::Frame::popup(ui.style());
    let inner = screen.width() - frame.total_margin().sum().x;

    egui::Area::new(ui.id().with("signal_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(above)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .show(ui.ctx(), |ui| {
            frame.show(ui, |ui| {
                ui.set_width(inner);
                tracks(app, ui, interval);
            });
        });
}

/// The eight rows and the span written under them.
fn tracks(app: &mut App, ui: &mut egui::Ui, interval: Duration) {
    let row = app.font.cell_size(ui.ctx()).y;
    let names: Vec<&str> = TRACKS.iter().map(|track| track.name).collect();
    let arrow = crate::ui::widgets::label_width(ui, &[icons::OUTGOING, icons::INCOMING]);
    let label = crate::ui::widgets::label_width(ui, &names) + LABEL_GAP + arrow + LABEL_GAP;
    let shown = fitting(ui.available_width() - label);

    let mut samples = std::mem::take(&mut app.ui.signal_samples);
    app.session.line_history(shown, &mut samples);

    for (index, track) in TRACKS.iter().enumerate() {
        if index > 0 && track.signal.outgoing() != TRACKS[index - 1].signal.outgoing() {
            ui.separator();
        }
        draw_track(ui, track, &samples, label, row);
    }

    if !samples.is_empty() {
        let span = interval * samples.len() as u32;
        ui.label(
            egui::RichText::new(format!(
                "{} {}",
                icons::INCOMING,
                crate::format::duration(span)
            ))
            .weak()
            .small(),
        );
    }

    app.ui.signal_samples = samples;
}

/// How many samples fit a track of that width.
///
/// One at the least, whatever is left: a window too narrow for the labels is a
/// window that draws them and one bar, and never a track of nothing wide.
fn fitting(width: f32) -> usize {
    ((width / BAR_WIDTH).floor().max(1.0)) as usize
}

/// One row: the name, the arrow of its direction, and the track.
fn draw_track(ui: &mut egui::Ui, track: &Track, samples: &[LineSample], label: f32, row: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row), egui::Sense::hover());
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let fill = match is_data(track.signal) {
        true => ui.visuals().selection.bg_fill,
        false => crate::ui::statusbar::level_color(ui, true),
    };
    let font = egui::TextStyle::Small.resolve(ui.style());
    let painter = ui.painter();

    painter.text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        track.name,
        font.clone(),
        text,
    );
    let arrow = match track.signal.outgoing() {
        true => icons::OUTGOING,
        false => icons::INCOMING,
    };
    painter.text(
        egui::pos2(rect.left() + label - LABEL_GAP, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        arrow,
        font,
        weak,
    );

    // The newest sample is pinned to the right edge, so a history that is still
    // filling grows leftwards instead of sliding under the eye.
    let lane = egui::Rect::from_min_max(
        egui::pos2(rect.left() + label, rect.top()),
        egui::pos2(rect.right(), rect.bottom()),
    );
    painter.rect_filled(lane, 0.0, weak.gamma_multiply(EMPTY_SHARE));

    let start = lane.right() - samples.len() as f32 * BAR_WIDTH;
    for run in runs(samples, track.signal) {
        let left = start + run.start as f32 * BAR_WIDTH;
        let right = start + run.end as f32 * BAR_WIDTH;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left.max(lane.left()), lane.top()),
                egui::pos2(right.max(lane.left()), lane.bottom()),
            ),
            0.0,
            fill,
        );
    }
}

/// Whether the row stands for bytes crossing rather than for a line.
fn is_data(signal: Signal) -> bool {
    matches!(signal, Signal::Sent | Signal::Received)
}

/// The stretches of the samples where the signal stood, as ranges of them.
///
/// Neighbours that say the same thing are one stretch, so a line that stood up
/// for a window as wide as the screen is one rectangle and not four hundred.
fn runs(samples: &[LineSample], signal: Signal) -> Vec<std::ops::Range<usize>> {
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        if !sample.has(signal) {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.end == index => last.end = index + 1,
            _ => runs.push(index..index + 1),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample with one signal standing.
    fn standing(signal: Signal) -> LineSample {
        LineSample::new(
            &zyt_serial::ControlLines {
                cts: signal == Signal::Cts,
                ..zyt_serial::ControlLines::default()
            },
            signal == Signal::Break,
            signal == Signal::Sent,
            signal == Signal::Received,
        )
    }

    /// The rows stand in two groups, what this side drives before what the peer
    /// does, and the data of each direction leads its own group.
    ///
    /// The plate is read as two blocks with one line between them, and a row
    /// that wandered into the wrong group would put that line in the middle of a
    /// direction.
    #[test]
    fn the_rows_group_what_this_side_drives_before_what_the_peer_drives() {
        let outgoing: Vec<&str> = TRACKS
            .iter()
            .filter(|track| track.signal.outgoing())
            .map(|track| track.name)
            .collect();
        let incoming: Vec<&str> = TRACKS
            .iter()
            .filter(|track| !track.signal.outgoing())
            .map(|track| track.name)
            .collect();

        assert_eq!(outgoing, ["TX", "BRK", "RTS", "DTR"]);
        assert_eq!(incoming, ["RX", "CTS", "DSR", "DCD"]);

        let groups = TRACKS
            .windows(2)
            .filter(|pair| pair[0].signal.outgoing() != pair[1].signal.outgoing())
            .count();
        assert_eq!(groups, 1, "one line between them and no other");
    }

    /// Every signal of a sample has a row, and no row draws the same signal
    /// twice: a plate that dropped one would be a plate quietly missing a line.
    #[test]
    fn every_signal_has_a_row_of_its_own() {
        for signal in Signal::ALL {
            let rows = TRACKS.iter().filter(|track| track.signal == signal).count();

            assert_eq!(rows, 1, "{signal:?}");
        }
        assert_eq!(TRACKS.len(), Signal::ALL.len());
    }

    /// Samples that say the same thing are one rectangle, and a stretch that
    /// ended is not joined to the next one.
    #[test]
    fn a_run_of_equal_samples_is_one_rectangle() {
        let up = standing(Signal::Cts);
        let down = LineSample::default();
        let samples = [up, up, up, down, down, up];

        assert_eq!(runs(&samples, Signal::Cts), vec![0..3, 5..6]);
        assert!(runs(&samples, Signal::Rts).is_empty());
        assert!(runs(&[], Signal::Cts).is_empty());
    }

    /// A signal that stood for every sample is one rectangle over the whole
    /// track, which is the case the merging is there for.
    #[test]
    fn a_signal_that_never_moved_is_one_rectangle() {
        let samples = vec![standing(Signal::Sent); 400];

        assert_eq!(runs(&samples, Signal::Sent), vec![0..400]);
    }

    /// The track holds as many samples as fit it, and never fewer than one: a
    /// window narrower than its own labels still draws a plate rather than
    /// asking for a track of nothing wide.
    #[test]
    fn a_track_holds_what_fits_it_and_never_nothing() {
        assert_eq!(fitting(BAR_WIDTH * 10.0), 10);
        assert_eq!(fitting(BAR_WIDTH * 10.5), 10, "half a bar is no bar");
        assert_eq!(fitting(0.0), 1);
        assert_eq!(fitting(-100.0), 1);
    }
}
