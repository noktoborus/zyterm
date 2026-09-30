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
//! It rises while the pointer rests on one of the letters the device drives, or
//! on the button of the flow control, and goes when the pointer leaves, because
//! it covers part of the output and a thing that covers what is being read has to
//! be the thing the hand is already doing. The right button on any of them leaves
//! it standing, and the palette carries the same switch — a line watched while
//! both hands are typing cannot be a line watched by holding a pointer still.
//!
//! `RTS` and `DTR` are not among them. Both of their buttons say what this side
//! does with the line — the left holds it, the right says which way — and a plate
//! that rose from them would rise every time one was worked.
//!
//! Which half it covers is decided by the cursor. It stands against the edge the
//! cursor is furthest from, so the rows being written into are the rows it never
//! hides: a shell at the top of a cleared screen is read under a plate at the
//! bottom, and a full screen with the cursor at its foot is read over a plate at
//! the top.
//!
//! Which rows stand is the device's own answer, the same `ShownLines` the row of
//! letters in the status bar is drawn from: the two are the same signals read two
//! ways, so a line switched off is off in both. The two directions of the data have
//! no letter to switch and are always drawn — they are what the tracks of the lines
//! are read against.
//!
//! Two of the rows are not signals but the queues of the driver, and they are
//! drawn as a share of the row rather than filled or empty: the fullest each
//! buffer has been seen is what the whole height means, so a bar at the top is
//! that buffer at the closest to full it has ever been. The scale grows as the
//! session runs, because how full is full is the one number no call of the driver
//! answers — it is only learned by seeing it.
//!
//! The two groups are the two directions, with a line between them. What this
//! side drives stands above what the peer drives, each with the data of its own
//! direction at the head of it, because the handshake is what leads to the
//! bytes.
//!
//! Everything but the three letters of a name is track. The labels and the
//! tracks stand in a grid of two columns, so the bars of every row begin at one
//! place and the span written under them begins where the oldest bar does — a
//! number in the corner of a plate names nothing, and this one names the left
//! edge of what is drawn beside it.

use crate::app::App;
use crate::ui::icons;
use std::time::Duration;
use zyt_serial::{LineSample, Signal};

/// Which queue of the driver a row draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Queue {
    /// Bytes the driver has taken off the line and nobody has read.
    Input,
    /// Bytes the driver has taken from this side and not put on the line.
    Output,
}

/// What a row of the plate draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Draw {
    /// A signal of the sample: the bar stands full where it stood.
    Signal(Signal),
    /// A queue of the driver: the bar stands as tall a share of the row as the
    /// queue is a share of the fullest it has been.
    Queue(Queue),
}

impl Draw {
    /// Whether this side is the one that drives or fills it.
    fn outgoing(self) -> bool {
        match self {
            Self::Signal(signal) => signal.outgoing(),
            Self::Queue(Queue::Output) => true,
            Self::Queue(Queue::Input) => false,
        }
    }
}

/// One row of the plate.
struct Track {
    /// The letters the row is called by, as the status bar calls them.
    name: &'static str,
    /// What it draws.
    draw: Draw,
}

/// The rows, in the order they stand.
///
/// What this side drives first, what the peer drives after it, and the data of
/// each direction at the head of its own group. The order is here and not in
/// `zyt-serial` because it is a question about reading a picture: the crate
/// knows which signals there are and which of them this side drives, and a
/// window is what knows the order they are read in.
const TRACKS: [Track; 12] = [
    Track {
        name: "TX",
        draw: Draw::Signal(Signal::Sent),
    },
    Track {
        name: "TXQ",
        draw: Draw::Queue(Queue::Output),
    },
    Track {
        name: "BRK",
        draw: Draw::Signal(Signal::Break),
    },
    Track {
        name: "HOLD",
        draw: Draw::Signal(Signal::Held),
    },
    Track {
        name: "RTS",
        draw: Draw::Signal(Signal::Rts),
    },
    Track {
        name: "DTR",
        draw: Draw::Signal(Signal::Dtr),
    },
    Track {
        name: "RX",
        draw: Draw::Signal(Signal::Received),
    },
    Track {
        name: "RXQ",
        draw: Draw::Queue(Queue::Input),
    },
    Track {
        name: "CTS",
        draw: Draw::Signal(Signal::Cts),
    },
    Track {
        name: "DSR",
        draw: Draw::Signal(Signal::Dsr),
    },
    Track {
        name: "DCD",
        draw: Draw::Signal(Signal::Carrier),
    },
    Track {
        name: "RI",
        draw: Draw::Signal(Signal::Ring),
    },
];

/// How wide one sample stands, in points.
///
/// One physical pixel, whatever the scale of the screen — which is the narrowest
/// a bar can be and the most history a track can hold. A stretch of samples that
/// agree is drawn as one rectangle, so what a reader looks for is as wide as it
/// lasted; it is a single sample standing alone that comes out one pixel wide.
///
/// It is answered in points because that is what the toolkit draws in, and taken
/// from the scale so that the edges of the bars land on the pixels of the screen.
/// A bar of a fixed number of points would be a bar of two pixels on one machine
/// and three on another.
fn bar_width(ui: &egui::Ui) -> f32 {
    1.0 / ui.ctx().pixels_per_point().max(1.0)
}

/// How far the plate stands from the bar it rises over.
const PLATE_GAP: f32 = 6.0;

/// Room between the name of a row and its track.
const LABEL_GAP: f32 = 6.0;

/// Room between two rows.
const ROW_GAP: f32 = 2.0;

/// What a track is painted in where its signal stood.
///
/// Four colours for four kinds of claim, so a row is read before its name is:
/// green is a line standing up, red is this side stopping the line carrying,
/// and the two directions of the data are told apart by being warm rather than
/// cool. They are written out here and not taken from the palette of the toolkit
/// because they have to mean the same thing in the light mode and the dark one,
/// which is what the green of [`crate::ui::statusbar::level_color`] already does;
/// all four stand at about the lightness of that green, so no row shouts over
/// its neighbours.
///
/// Red for the break and for the hold on the reading: both are a thing this side
/// does that stops the line carrying, one in each direction, and both are states
/// somebody wants to spot at a glance in a picture they opened because the line
/// went quiet.
const STOPPED: egui::Color32 = egui::Color32::from_rgb(0xd0, 0x5c, 0x5c);
/// Bytes on their way out.
const SENT: egui::Color32 = egui::Color32::from_rgb(0xd8, 0x8c, 0x3c);
/// Bytes on their way in.
const RECEIVED: egui::Color32 = egui::Color32::from_rgb(0xd0, 0xc0, 0x48);

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
    if !standing(app) {
        return;
    }

    // The plate is a picture of a clock that runs whether a frame is drawn or
    // not, so it asks for one per sample while it stands and for nothing once
    // it is gone. Nowhere else asks.
    let interval = app.session.lines_interval();
    ui.ctx().request_repaint_after(interval);

    let screen = ui.ctx().content_rect();
    let frame = egui::Frame::popup(ui.style());
    let inner = screen.width() - frame.total_margin().sum().x;
    // The top of the status bar is the foot of the terminal, and the content
    // area begins at its head: no panel stands above it.
    let (anchor, pivot) = match at_top(&app.session.content) {
        true => (
            egui::pos2(screen.left(), screen.top() + PLATE_GAP),
            egui::Align2::LEFT_TOP,
        ),
        false => (
            egui::pos2(screen.left(), ui.max_rect().top() - PLATE_GAP),
            egui::Align2::LEFT_BOTTOM,
        ),
    };

    egui::Area::new(ui.id().with("signal_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(anchor)
        .pivot(pivot)
        .show(ui.ctx(), |ui| {
            frame.show(ui, |ui| {
                ui.set_width(inner);
                tracks(app, ui, interval);
            });
        });
}

/// The rows and the span written under them, as a grid of two columns.
///
/// The names take exactly the width of the widest of them and the tracks take
/// everything left, which is what puts as many samples on the screen as the
/// screen can hold.
fn tracks(app: &mut App, ui: &mut egui::Ui, interval: Duration) {
    let row = app.font.cell_size(ui.ctx()).y;
    let shows = app.shown_lines();
    let drawn: Vec<&Track> = TRACKS
        .iter()
        .filter(|track| match track.draw {
            Draw::Signal(signal) => shows.draws(signal),
            Draw::Queue(_) => true,
        })
        .collect();
    let names: Vec<&str> = drawn.iter().map(|track| track.name).collect();
    let label = crate::ui::widgets::label_width(ui, &names);
    let lane = (ui.available_width() - label - LABEL_GAP).max(0.0);
    let bar = bar_width(ui);
    let shown = fitting(lane, bar);

    let mut samples = std::mem::take(&mut app.ui.signal_samples);
    let scale = app.session.line_history(shown, &mut samples);

    egui::Grid::new(ui.make_persistent_id("signal_tracks"))
        .num_columns(2)
        .spacing([LABEL_GAP, ROW_GAP])
        .show(ui, |ui| {
            for (index, track) in drawn.iter().enumerate() {
                if index > 0 && track.draw.outgoing() != drawn[index - 1].draw.outgoing() {
                    ui.label("");
                    ui.separator();
                    ui.end_row();
                }
                ui.label(track.name);
                draw_lane(ui, track, &samples, lane, row, bar, scale);
                ui.end_row();
            }

            if !samples.is_empty() {
                ui.label("");
                draw_span(ui, &samples, lane, interval, bar);
                ui.end_row();
            }
        });

    app.ui.signal_samples = samples;
}

/// Whether the plate is drawn at all.
///
/// Pinned it stands whatever the pointer is doing; otherwise it stands while the
/// pointer is on the row and has not just pressed the plate away.
///
/// A session on no line has nothing to draw. Its history is empty, so what a
/// plate would show is a column of empty tracks and no span — and a pin set over
/// a device stays set, so it would show that for every console opened after it.
fn standing(app: &App) -> bool {
    if !app.session.is_serial() {
        return false;
    }
    app.ui.signals_pinned || (app.ui.signals_hovered && !app.ui.signals_hidden)
}

/// Whether the plate stands against the head of the terminal rather than its
/// foot.
///
/// It stands away from the cursor: the rows being written into are the rows
/// somebody is reading, and they are the ones a plate must not cover. A cursor
/// in the lower half puts the plate at the top, and one in the upper half puts
/// it at the bottom.
///
/// A page with no cursor at all leaves it at the foot, which is where it was
/// before there was anything to dodge and where the letters that raise it stand.
///
/// The shape of the cursor is not asked about. A program that hid it is still
/// writing where it stands, and a plate that moved because a cursor stopped
/// being drawn would move for a thing nobody can see.
fn at_top(content: &zyt_term::RenderableContent) -> bool {
    let Some(cursor) = content.cursor else {
        return false;
    };
    cursor.row * 2 >= content.rows
}

/// How many samples fit a track of that width.
///
/// One at the least, whatever is left: a window too narrow for the labels is a
/// window that draws them and one bar, and never a track of nothing wide.
fn fitting(width: f32, bar: f32) -> usize {
    ((width / bar.max(f32::EPSILON)).floor().max(1.0)) as usize
}

/// The track of one row.
///
/// A signal stands the whole height of the row or none of it, because a level is
/// one or the other. A queue stands the share of the row it is of the fullest it
/// has ever been, from the bottom up, so the tallest bar of a track is the moment
/// that buffer was the closest to full it has been seen.
fn draw_lane(
    ui: &mut egui::Ui,
    track: &Track,
    samples: &[LineSample],
    width: f32,
    row: f32,
    bar: f32,
    scale: zyt_serial::LineScale,
) {
    let (lane, _) = ui.allocate_exact_size(egui::vec2(width, row), egui::Sense::hover());
    let weak = ui.visuals().weak_text_color();
    let fill = fill_color(ui, track.draw);
    let painter = ui.painter();

    painter.rect_filled(lane, 0.0, weak.gamma_multiply(EMPTY_SHARE));

    let start = oldest_bar(&lane, samples.len(), bar);
    for (run, share) in stretches(samples, track.draw, scale) {
        let left = (start + run.start as f32 * bar).max(lane.left());
        let right = (start + run.end as f32 * bar).max(lane.left());
        let top = lane.bottom() - lane.height() * share;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, lane.bottom())),
            0.0,
            fill,
        );
    }
}

/// How far back the track reaches, written where it reaches back to.
///
/// It stands under the oldest bar and not under the left edge of the lane,
/// because those two are not the same place while the history is still filling:
/// the number names the bar it begins at, and a number standing away from what
/// it names is a number about nothing.
fn draw_span(ui: &mut egui::Ui, samples: &[LineSample], width: f32, interval: Duration, bar: f32) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let height = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let (lane, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let span = interval * samples.len() as u32;
    let text = format!("{} {}", icons::EARLIER, crate::format::duration(span));

    ui.painter().text(
        egui::pos2(oldest_bar(&lane, samples.len(), bar), lane.top()),
        egui::Align2::LEFT_TOP,
        text,
        font,
        ui.visuals().weak_text_color(),
    );
}

/// Where the oldest bar of a track begins.
///
/// The newest sample is pinned to the right edge, so a history that is still
/// filling grows leftwards instead of sliding under the eye. The right edge is
/// brought to a whole bar first: every bar is one pixel and every step is one
/// pixel, so an edge that began between two of them would leave every bar of
/// every row smeared over two.
fn oldest_bar(lane: &egui::Rect, samples: usize, bar: f32) -> f32 {
    let step = bar.max(f32::EPSILON);
    let right = (lane.right() / step).round() * step;
    (right - samples as f32 * step).max(lane.left())
}

/// What the stretches of this row are painted in.
///
/// A queue wears the colour of the data of its own direction: it is the same
/// bytes a moment earlier or a moment later, and the row beside it is what it is
/// read against.
fn fill_color(ui: &egui::Ui, draw: Draw) -> egui::Color32 {
    match draw {
        Draw::Signal(Signal::Sent) | Draw::Queue(Queue::Output) => SENT,
        Draw::Signal(Signal::Received) | Draw::Queue(Queue::Input) => RECEIVED,
        Draw::Signal(Signal::Break | Signal::Held) => STOPPED,
        Draw::Signal(_) => crate::ui::statusbar::level_color(ui, true),
    }
}

/// The stretches of a row, each with the share of the height it stands at.
///
/// Neighbours that say the same thing are one stretch, whichever kind of row it
/// is: a line that stood up for a window as wide as the screen is one rectangle,
/// and so is a queue that sat at one depth.
///
/// A queue with nothing behind it — a buffer never seen to hold a byte — stands
/// at nothing rather than dividing by it.
fn stretches(
    samples: &[LineSample],
    draw: Draw,
    scale: zyt_serial::LineScale,
) -> Vec<(std::ops::Range<usize>, f32)> {
    let share = |sample: &LineSample| match draw {
        Draw::Signal(signal) => f32::from(sample.has(signal)),
        Draw::Queue(Queue::Input) => depth(sample.input_queue(), scale.input),
        Draw::Queue(Queue::Output) => depth(sample.output_queue(), scale.output),
    };

    let mut stretches: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        let standing = share(sample);
        if standing <= 0.0 {
            continue;
        }
        match stretches.last_mut() {
            Some((run, held)) if run.end == index && *held == standing => run.end = index + 1,
            _ => stretches.push((index..index + 1, standing)),
        }
    }
    stretches
}

/// How much of a row a queue that deep stands at.
fn depth(held: u16, fullest: u16) -> f32 {
    match fullest {
        0 => 0.0,
        fullest => f32::from(held) / f32::from(fullest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample with one signal standing.
    fn standing(signal: Signal) -> LineSample {
        LineSample::new(
            &zyt_serial::ControlLines {
                cts: signal == Signal::Cts,
                ri: signal == Signal::Ring,
                ..zyt_serial::ControlLines::default()
            },
            signal == Signal::Break,
            signal == Signal::Held,
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
            .filter(|track| track.draw.outgoing())
            .map(|track| track.name)
            .collect();
        let incoming: Vec<&str> = TRACKS
            .iter()
            .filter(|track| !track.draw.outgoing())
            .map(|track| track.name)
            .collect();

        assert_eq!(outgoing, ["TX", "TXQ", "BRK", "HOLD", "RTS", "DTR"]);
        assert_eq!(incoming, ["RX", "RXQ", "CTS", "DSR", "DCD", "RI"]);

        let groups = TRACKS
            .windows(2)
            .filter(|pair| pair[0].draw.outgoing() != pair[1].draw.outgoing())
            .count();
        assert_eq!(groups, 1, "one line between them and no other");
    }

    /// Every signal of a sample has a row, and so has every queue, and no row
    /// draws the same thing twice: a plate that dropped one would be a plate
    /// quietly missing a line.
    #[test]
    fn everything_a_sample_carries_has_a_row_of_its_own() {
        let mut drawn: Vec<Draw> = Signal::ALL.into_iter().map(Draw::Signal).collect();
        drawn.push(Draw::Queue(Queue::Input));
        drawn.push(Draw::Queue(Queue::Output));

        for one in &drawn {
            let rows = TRACKS.iter().filter(|track| track.draw == *one).count();

            assert_eq!(rows, 1, "{one:?}");
        }
        assert_eq!(TRACKS.len(), drawn.len());
    }

    /// A row of a signal, as the ranges it stands over.
    fn ranges(
        samples: &[LineSample],
        draw: Draw,
        scale: zyt_serial::LineScale,
    ) -> Vec<std::ops::Range<usize>> {
        stretches(samples, draw, scale)
            .into_iter()
            .map(|(run, _)| run)
            .collect()
    }

    /// Samples that say the same thing are one rectangle, and a stretch that
    /// ended is not joined to the next one.
    #[test]
    fn a_run_of_equal_samples_is_one_rectangle() {
        let up = standing(Signal::Cts);
        let down = LineSample::default();
        let samples = [up, up, up, down, down, up];
        let none = zyt_serial::LineScale::default();

        assert_eq!(
            ranges(&samples, Draw::Signal(Signal::Cts), none),
            vec![0..3, 5..6]
        );
        assert!(ranges(&samples, Draw::Signal(Signal::Rts), none).is_empty());
        assert!(ranges(&[], Draw::Signal(Signal::Cts), none).is_empty());
    }

    /// A signal that stood for every sample is one rectangle over the whole
    /// track, which is the case the merging is there for.
    #[test]
    fn a_signal_that_never_moved_is_one_rectangle() {
        let samples = vec![standing(Signal::Sent); 400];

        assert_eq!(
            ranges(
                &samples,
                Draw::Signal(Signal::Sent),
                zyt_serial::LineScale::default()
            ),
            vec![0..400]
        );
    }

    /// A queue stands the share of the row it is of the fullest it has been, and
    /// the fullest of all stands the whole of it.
    #[test]
    fn a_queue_stands_at_its_share_of_the_fullest_it_has_been() {
        let scale = zyt_serial::LineScale {
            input: 400,
            output: 0,
        };
        let samples: Vec<LineSample> = [0usize, 100, 200, 400]
            .into_iter()
            .map(|held| LineSample::default().with_queues(held, 0))
            .collect();

        let shares: Vec<f32> = stretches(&samples, Draw::Queue(Queue::Input), scale)
            .into_iter()
            .map(|(_, share)| share)
            .collect();

        assert_eq!(
            shares,
            vec![0.25, 0.5, 1.0],
            "and nothing for the empty one"
        );
    }

    /// A queue never seen to hold a byte stands at nothing rather than dividing
    /// by it.
    #[test]
    fn a_queue_with_no_scale_behind_it_stands_at_nothing() {
        let samples = [LineSample::default().with_queues(7, 7)];
        let none = zyt_serial::LineScale::default();

        assert!(stretches(&samples, Draw::Queue(Queue::Input), none).is_empty());
        assert!(stretches(&samples, Draw::Queue(Queue::Output), none).is_empty());
        assert_eq!(depth(0, 0), 0.0);
        assert_eq!(depth(5, 0), 0.0);
    }

    /// Two stretches at one depth are one rectangle, and a depth that changed
    /// begins another.
    #[test]
    fn a_queue_that_sat_at_one_depth_is_one_rectangle() {
        let scale = zyt_serial::LineScale {
            input: 10,
            output: 0,
        };
        let samples: Vec<LineSample> = [5usize, 5, 5, 10, 5]
            .into_iter()
            .map(|held| LineSample::default().with_queues(held, 0))
            .collect();

        assert_eq!(
            ranges(&samples, Draw::Queue(Queue::Input), scale),
            vec![0..3, 3..4, 4..5]
        );
    }

    /// A page of that many rows with the cursor on that row.
    fn page(rows: usize, row: usize) -> zyt_term::RenderableContent {
        zyt_term::RenderableContent {
            rows,
            cursor: Some(zyt_term::CursorInfo {
                column: 0,
                row,
                shape: zyt_term::CursorShape::Block,
            }),
            ..zyt_term::RenderableContent::default()
        }
    }

    /// The plate stands against the edge the cursor is furthest from, so the
    /// rows being written into are the rows it never covers.
    ///
    /// The halves meet in the middle, and a page of an odd number of rows has to
    /// fall one way or the other rather than panicking on the row between them.
    #[test]
    fn the_plate_stands_away_from_the_cursor() {
        assert!(!at_top(&page(24, 0)), "the head of the page sends it down");
        assert!(
            !at_top(&page(24, 11)),
            "and so does the row above the middle"
        );
        assert!(at_top(&page(24, 12)), "the middle row sends it up");
        assert!(at_top(&page(24, 23)), "and so does the foot");

        assert!(!at_top(&page(25, 12)), "an odd page falls one way");
        assert!(at_top(&page(25, 13)), "and the row after it the other");

        assert!(!at_top(&page(1, 0)));
    }

    /// The newest bar stands at the right edge whatever the history holds, so a
    /// track that is still filling grows leftwards instead of sliding under the
    /// eye — and a track holding more than fits it never begins outside itself.
    #[test]
    fn the_newest_bar_stands_at_the_right_edge_of_the_track() {
        let bar = 1.0;
        let lane = egui::Rect::from_min_max(egui::pos2(20.0, 0.0), egui::pos2(30.0, 8.0));

        assert_eq!(
            oldest_bar(&lane, 10, bar),
            lane.left(),
            "a full track begins at its edge"
        );
        assert_eq!(
            oldest_bar(&lane, 4, bar),
            lane.right() - bar * 4.0,
            "and a filling one hangs from the right"
        );
        assert_eq!(
            oldest_bar(&lane, 0, bar),
            lane.right(),
            "nothing begins at the end"
        );
        assert_eq!(
            oldest_bar(&lane, 99, bar),
            lane.left(),
            "and more than fits never begins outside the track"
        );
    }

    /// Each kind of claim is painted in its own colour, and the lines share one.
    ///
    /// A row is read before its name is, so the two the peer drives and the two
    /// this side stops the line with must not come out the same: a picture opened
    /// because the line went quiet is read by looking for the red.
    #[test]
    fn every_kind_of_row_is_painted_in_its_own_colour() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let green = crate::ui::statusbar::level_color(ui, true);

            assert_eq!(fill_color(ui, Draw::Signal(Signal::Sent)), SENT);
            assert_eq!(fill_color(ui, Draw::Signal(Signal::Received)), RECEIVED);
            assert_eq!(fill_color(ui, Draw::Signal(Signal::Break)), STOPPED);
            assert_eq!(fill_color(ui, Draw::Signal(Signal::Held)), STOPPED);
            for line in [
                Signal::Rts,
                Signal::Dtr,
                Signal::Cts,
                Signal::Dsr,
                Signal::Carrier,
                Signal::Ring,
            ] {
                assert_eq!(fill_color(ui, Draw::Signal(line)), green, "{line:?}");
            }

            // A queue wears the colour of the data of its own direction: it is
            // the same bytes a moment earlier or later, and the row beside it is
            // what it is read against.
            assert_eq!(fill_color(ui, Draw::Queue(Queue::Output)), SENT);
            assert_eq!(fill_color(ui, Draw::Queue(Queue::Input)), RECEIVED);

            let four = [SENT, RECEIVED, STOPPED, green];
            for (index, colour) in four.iter().enumerate() {
                for other in &four[index + 1..] {
                    assert_ne!(colour, other, "two kinds of row share a colour");
                }
            }
        });
        output.textures_delta.clear();
    }

    /// The bars sit on the pixels of the screen.
    ///
    /// Every bar is one pixel wide and every step is one pixel, so the whole of
    /// a row is crisp or none of it is: a right edge that began between two
    /// pixels would smear every bar of every track over two of them.
    #[test]
    fn the_bars_begin_on_a_pixel_of_the_screen() {
        let bar = 0.5;
        let lane = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.3, 8.0));

        let start = oldest_bar(&lane, 8, bar);
        let bars = start / bar;

        assert!(
            (bars - bars.round()).abs() < 1e-4,
            "{start} is off the grid"
        );
    }

    /// A page with no cursor leaves the plate at the foot, which is where it
    /// stood before there was anything to dodge.
    #[test]
    fn a_page_with_no_cursor_leaves_the_plate_where_it_was() {
        assert!(!at_top(&zyt_term::RenderableContent::default()));
    }

    /// The track holds as many samples as fit it, and never fewer than one: a
    /// window narrower than its own labels still draws a plate rather than
    /// asking for a track of nothing wide.
    #[test]
    fn a_track_holds_what_fits_it_and_never_nothing() {
        assert_eq!(fitting(10.0, 1.0), 10, "one bar to a pixel");
        assert_eq!(
            fitting(10.0, 0.5),
            20,
            "and twice as many where a pixel is half a point"
        );
        assert_eq!(fitting(10.5, 1.0), 10, "half a bar is no bar");
        assert_eq!(fitting(0.0, 1.0), 1);
        assert_eq!(fitting(-100.0, 1.0), 1);
    }
}
