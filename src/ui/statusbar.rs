//! Status bar: connection, line parameters, modem lines, transfer mode.

use crate::app::App;
use crate::commands::AppCommand;
use crate::session::ConnectionState;
use crate::ui::PLATE_GAP;
use crate::ui::icons;
use rust_i18n::t;

/// Draws the status bar.
///
/// In the settings it carries the way back and the gear and nothing else: the
/// line parameters, the modem lines and what is running all name the connection
/// the settings are not showing, and a control that acts on something out of
/// sight is a control nobody asked for.
pub fn draw(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    ui.horizontal(|ui| {
        connection_label(app, ui, context);

        if app.ui.view == crate::app::MainView::Settings {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                gear(app, ui, context);
            });
            return;
        }

        ui.separator();

        if app.session.is_serial() {
            line_params(app, ui);
            ui.separator();
            modem_lines(app, ui);
        } else {
            // A console has no lines, and the two controls that are not about a
            // line are the two it does have: what it has been doing, and the hold
            // on the reading of it.
            signals_button(app, ui);
            hold_switch(app, ui);
        }
        ui.separator();

        crate::ui::signals::plate(app, ui);
        transfer_time(app, ui);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            gear(app, ui, context);
            search_button(app, ui, context);
            history_button(app, ui, context);
            progress_bar(app, ui);
            tasks_control(app, ui);
            line_state(app, ui);
        });
    });
}

/// The way into the settings and back out of them.
fn gear(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    let in_settings = app.ui.view == crate::app::MainView::Settings;
    let hint = if in_settings {
        t!("settings.back")
    } else {
        t!("menu.settings")
    };
    if ui
        .add(egui::Button::new(icons::SETTINGS).selected(in_settings))
        .on_hover_text(hint)
        .clicked()
    {
        app.run_command(AppCommand::SettingsOpen, context);
    }
}

/// The way into the search, left of the gear.
///
/// The bar of the search takes the place of this one, so the button is gone for
/// as long as the search is open: what it would open is already there.
fn search_button(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    if app.ui.view != crate::app::MainView::Terminal {
        return;
    }
    if ui
        .button(icons::SEARCH)
        .on_hover_text(t!("command.search.open"))
        .clicked()
    {
        app.run_command(AppCommand::SearchOpen, context);
    }
}

/// The commands to type back, left of the gear: the ones the shell of this
/// source marked and the ones added by hand.
///
/// A window with neither shows no button: an empty list is nothing to open, and
/// a button that does nothing when it is pressed says less than no button at
/// all.
fn history_button(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    if !app.has_command_history() {
        return;
    }
    if ui
        .button(icons::HISTORY)
        .on_hover_text(t!("history.hint"))
        .clicked()
    {
        app.run_command(AppCommand::HistoryOpen, context);
    }
}

fn connection_label(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    if app.ui.view == crate::app::MainView::Settings {
        let response = ui.button(format!("{} {}", icons::BACK, t!("settings.back")));
        app.ui.status_bar_id = Some(response.id);
        if app.ui.status_bar_focus_wanted {
            app.ui.status_bar_focus_wanted = false;
            response.request_focus();
        }
        if response.on_hover_text(t!("settings.back_hint")).clicked() {
            app.run_command(AppCommand::SettingsOpen, context);
        }
        return;
    }

    // A button with a session behind it carries no hint. What it opens is a
    // menu, which says what it offers by standing open, and the words that used
    // to hang here said that a menu is a menu. The plate of the times is what
    // the pointer finds there instead. With nothing connected the button does
    // one thing and does it without a menu, so that one thing is named.
    let console = app.session.console_shown().map(str::to_string);
    let (text, color, hint) = match app.session.state() {
        ConnectionState::Connected => (
            app.session
                .port_path()
                .map(str::to_string)
                .or(console)
                .unwrap_or_else(|| t!("status.connected").to_string()),
            egui::Color32::from_rgb(0x5c, 0xb8, 0x5c),
            None,
        ),
        ConnectionState::Opening => (
            t!("status.opening").to_string(),
            ui.visuals().warn_fg_color,
            None,
        ),
        ConnectionState::Waiting => (
            t!(
                "status.waiting",
                port = app.session.port_path().unwrap_or("-")
            )
            .to_string(),
            ui.visuals().warn_fg_color,
            None,
        ),
        ConnectionState::Idle => (
            t!("status.disconnected").to_string(),
            ui.visuals().weak_text_color(),
            Some(match app.previous_source() {
                Some(_) => t!("status.reconnect"),
                None => t!("status.select_port"),
            }),
        ),
    };

    let response = ui.add(egui::Button::new(egui::RichText::new(text).color(color)));
    app.ui.status_bar_id = Some(response.id);
    if app.ui.status_bar_focus_wanted {
        app.ui.status_bar_focus_wanted = false;
        response.request_focus();
    }
    let response = match hint {
        Some(hint) => response.on_hover_text(hint),
        None => response,
    };

    // The right button is what leaves the plate standing, because the left one
    // is already the menu of the session: the plate is read while the terminal
    // is worked in, and a session menu opening every time somebody wants to see
    // the times is a menu in the way.
    //
    // The right button that takes it down has to take it down where it is
    // pressed, and it is pressed on the thing the pointer is resting on, which
    // is what would open the plate again on the same frame. So the plate is
    // held down until that pointer leaves: pressed twice, the button closes the
    // plate and keeps it closed, and the pointer arriving again opens it again.
    if app.session.has_source() {
        if response.secondary_clicked() {
            app.ui.data_plate_pinned = !app.ui.data_plate_pinned;
            app.ui.data_plate_hidden = !app.ui.data_plate_pinned;
        }
        if !response.hovered() {
            app.ui.data_plate_hidden = false;
        }

        let standing =
            app.ui.data_plate_pinned || (response.hovered() && !app.ui.data_plate_hidden);
        if standing && data_plate(app, ui) {
            app.ui.data_plate_pinned = false;
        }
        if app.ui.data_plate_pinned && clicked_beside_plate(app, ui, response.rect) {
            app.ui.data_plate_pinned = false;
        }
    }

    if response.clicked() {
        if app.session.has_source() {
            app.open_session_menu();
        } else if app.previous_source().is_some() {
            app.reconnect_previous();
        } else {
            app.open_source_menu();
        }
    }

    session_kind(app, ui);
    mouse_grab(app, ui, context);
    captured_sign(app, ui);
}

/// The plate of the times, in the bottom left corner of the window.
///
/// It rises there and not under the pointer because it is read beside the
/// terminal and not instead of it: a plate that follows the pointer covers
/// whatever the pointer was reaching for, and this one is looked at while the
/// output above it goes on.
///
/// Answers true when it was pressed, which is what takes it down: a plate left
/// standing has to be dismissable where it stands, and the thing under the
/// pointer is the plate itself and not the button that opened it.
fn data_plate(app: &mut App, ui: &mut egui::Ui) -> bool {
    // The numbers here that count from now — how long ago each moment was, and
    // the silence over the last stretch of the track — move by themselves, and
    // a line that says nothing wakes no frame of its own. So the plate asks for
    // a frame once in a while and for nothing in between, and nothing at all
    // while there is no data to count from. Once the plate is gone it asks for
    // nothing, because this is the only place that asks.
    if let Some(data) = app.session.last_data() {
        ui.ctx().request_repaint_after(next_tick(data.elapsed()));
    }

    let screen = ui.ctx().content_rect();
    let above = egui::pos2(screen.left() + PLATE_GAP, ui.max_rect().top() - PLATE_GAP);

    let plate = egui::Area::new(ui.id().with("data_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(above)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                if let (Some(data), Some(first)) =
                    (app.session.last_data(), app.session.first_data())
                {
                    delta_track(ui, data, first, app.session.last_written());
                    ui.separator();
                }
                egui::Grid::new("data_plate_rows")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (name, at, ago) in data_rows(
                            app.session.last_data(),
                            app.session.first_data(),
                            app.session.last_written(),
                            app.session.answered(),
                        ) {
                            ui.label(name);
                            ui.label(egui::RichText::new(at).strong());
                            ui.label(egui::RichText::new(ago).weak());
                            ui.end_row();
                        }
                    });
            });
        });

    app.ui.data_plate_rect = plate.response.rect;
    plate
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// True when this frame carried a press that landed neither on the plate of
/// the times nor on the button that opens it.
///
/// A plate left standing is put away by a press beside it, the way the panel of
/// what is running is: two things that stand over the terminal until they are
/// dismissed are dismissed the same way. The button is left out because the
/// press on it already turns the plate over.
fn clicked_beside_plate(app: &App, ui: &egui::Ui, button: egui::Rect) -> bool {
    clicked_beside(ui, app.ui.data_plate_rect, button)
}

/// True when this frame carried a press that landed neither on that plate nor
/// on the button that opens it.
fn clicked_beside(ui: &egui::Ui, plate: egui::Rect, button: egui::Rect) -> bool {
    ui.ctx().input(|input| {
        let Some(at) = input.pointer.interact_pos() else {
            return false;
        };
        input.pointer.any_click() && !plate.contains(at) && !button.contains(at)
    })
}

/// How long until the plate asks for a frame again.
///
/// The numbers of it that count from now — how long ago each moment was, and
/// the silence since the data stopped — move whether a frame is drawn for them
/// or not, and a plate that kept up with what they say would ask for a frame
/// every millisecond while they are written in milliseconds. They are read by
/// somebody watching a line go quiet: what that reading is to the millisecond
/// is not what they came for, and a frame a millisecond of a window that has
/// nothing else to do is the whole cost of a frame paid for a digit.
///
/// So it asks once in [`PLATE_TICK`] and the number jumps by that much. The
/// wait is to the next step and not a step long, so the steps fall on the same
/// moments whenever the plate is opened, and a plate opened just before one
/// does not sit through a whole step before its first.
fn next_tick(span: std::time::Duration) -> std::time::Duration {
    let step = PLATE_TICK;
    let rest = std::time::Duration::from_nanos((span.as_nanos() % step.as_nanos()) as u64);
    (step - rest).max(std::time::Duration::from_millis(1))
}

/// How often the plate of the times asks for a frame.
const PLATE_TICK: std::time::Duration = std::time::Duration::from_secs(10);

/// How wide the track of the answer stands, and how thick.
const TRACK_WIDTH: f32 = 280.0;
/// Thickness of the stretches painted under the line of the track.
const TRACK_HEIGHT: f32 = 6.0;
/// Radius of a moment on the line, which is also the room kept at either end so
/// the first and the last of them are not cut in half.
const DOT: f32 = 4.0;
/// Room between two rows of the track, and between two names on one row.
const ROW_GAP: f32 = 2.0;

/// One stretch of the track: how long it lasted and what it is painted in.
struct Stretch {
    span: std::time::Duration,
    fill: egui::Color32,
}

/// One moment of the track, under the dot that stands for it.
///
/// Several names where several of the moments fell together: a write answered
/// in no time at all and the byte that answered it are one place on a line, and
/// two dots at one place would be one dot with one of the names lost behind it.
struct Mark {
    /// Where it stands, which is also what says that two of them are one.
    since: std::time::Instant,
    /// The clock reading written over the names, or nothing for the arrow.
    time: String,
    /// What it is called, one name per moment that fell here.
    names: Vec<String>,
    /// Whether the data had begun by then, which says what the stretch leaving
    /// this mark measures: the wait before the answer, or the answer itself.
    began: bool,
}

/// The moments the track carries, in the order they happened, one mark per
/// place on the line.
///
/// Data that began and ended at the same moment is named once and by its end:
/// one chunk of bytes is one moment, and a dot saying that the data began and
/// ended there says twice what happened once.
///
/// A write that came after the data it stands beside answered nothing, so it is
/// left out: what it would draw is a stretch running backwards.
fn track_marks(
    data: crate::session::Moment,
    first: crate::session::Moment,
    input: Option<crate::session::Moment>,
) -> Vec<Mark> {
    let asked = input.filter(|input| first.since >= input.since);
    let named = [
        (asked, "status.track_input", false),
        (
            Some(first).filter(|first| first.since != data.since),
            "status.track_first",
            true,
        ),
        (Some(data), "status.track_last", true),
    ];

    let mut marks: Vec<Mark> = Vec::new();
    for (moment, key, began) in named {
        let Some(moment) = moment else {
            continue;
        };
        let name = t!(key).to_string();
        match marks.last_mut() {
            Some(mark) if mark.since == moment.since => {
                mark.names.push(name);
                mark.began |= began;
            }
            _ => marks.push(Mark {
                since: moment.since,
                time: crate::format::clock(moment.at),
                names: vec![name],
                began,
            }),
        }
    }

    marks
}

/// How many rows of text the names of this mark and the clock over them take.
fn mark_rows(mark: &Mark) -> usize {
    let lines: usize = mark.names.iter().map(|name| name.lines().count()).sum();
    let between = mark.names.len().saturating_sub(1);
    usize::from(!mark.time.is_empty()) + lines + between
}

/// The track of the answer: the moments on a line, the stretches between them
/// painted under it, the spans written over it.
///
/// The numbers of the plate say how long each stretch was and the track says
/// what happened in which order, which is the thing a column of numbers cannot
/// show: which moments fell together, which stretch is the wait and which is
/// the answer, and that the line has said nothing since.
///
/// It is not a scale. Every stretch that is over takes the same quarter of it
/// whatever it lasted, and the silence takes what is left, because a wait of
/// fifty milliseconds beside a silence of five minutes drawn to scale is no
/// stretch at all, and the one thing the picture must never say is that
/// something did not happen. The number over each stretch is the true span.
fn delta_track(
    ui: &mut egui::Ui,
    data: crate::session::Moment,
    first: crate::session::Moment,
    input: Option<crate::session::Moment>,
) {
    let visuals = ui.visuals();
    let (font, small) = (
        egui::TextStyle::Small.resolve(ui.style()),
        egui::TextStyle::Small.resolve(ui.style()),
    );
    let line = visuals.weak_text_color();
    let text = visuals.text_color();

    let mut marks = track_marks(data, first, input);
    let mut stretches: Vec<Stretch> = marks
        .windows(2)
        .map(|pair| Stretch {
            span: pair[1].since.saturating_duration_since(pair[0].since),
            fill: if pair[0].began {
                visuals.selection.bg_fill
            } else {
                visuals.warn_fg_color.gamma_multiply(0.7)
            },
        })
        .collect();

    stretches.push(Stretch {
        span: data.elapsed(),
        fill: line.gamma_multiply(0.4),
    });
    marks.push(Mark {
        since: data.since,
        time: String::new(),
        names: vec![t!("status.track_now").to_string()],
        began: true,
    });

    let label_rows = marks.iter().map(mark_rows).max().unwrap_or(1);
    let written = font.size + small.size * label_rows as f32 + ROW_GAP * 3.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(TRACK_WIDTH, written + TRACK_HEIGHT + DOT * 2.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();

    let edges = stretch_edges(stretches.len(), rect.left() + DOT, rect.right() - DOT);

    let axis = rect.top() + font.size + ROW_GAP * 2.0;
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left() + DOT, axis + DOT),
        egui::pos2(rect.right() - DOT, axis + DOT + TRACK_HEIGHT),
    );

    // The spans over the track, each in the middle of what it measures, moved
    // aside where the middle is taken and left out only where nothing is free.
    // The number is the one somebody came to read, so it is written beside its
    // stretch rather than dropped for standing over a short one.
    let mut taken = rect.left();
    for (index, stretch) in stretches.iter().enumerate() {
        let (left, right) = (edges[index], edges[index + 1]);
        let span = crate::format::duration(stretch.span);
        let galley = painter.layout_no_wrap(span, font.clone(), text);
        let width = galley.rect.width();
        let at = ((left + right) / 2.0 - width / 2.0)
            .max(taken)
            .min(rect.right() - width);
        if at < taken {
            continue;
        }
        taken = at + width + ROW_GAP * 2.0;
        painter.galley(egui::pos2(at, rect.top()), galley, text);
    }

    painter.line_segment(
        [egui::pos2(bar.left(), axis), egui::pos2(bar.right(), axis)],
        egui::Stroke::new(1.0, line),
    );
    for (index, stretch) in stretches.iter().enumerate() {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(edges[index], bar.top()),
                egui::pos2(edges[index + 1], bar.bottom()),
            ),
            0.0,
            stretch.fill,
        );
    }

    // The moments under the track, and the same rule: one that would stand on
    // the one beside it is left out, because two names run together say less
    // than one name does.
    let mut taken = rect.left() - ROW_GAP;
    for (index, mark) in marks.iter().enumerate() {
        let at = edges[index.min(edges.len() - 1)];
        // Now is the one moment that has not happened yet, so it is the head of
        // an arrow and not a dot: the line runs into it and does not stop.
        let last = index + 1 == marks.len();
        if last {
            let head = egui::pos2(rect.right(), axis);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    head,
                    egui::pos2(head.x - DOT * 1.6, axis - DOT),
                    egui::pos2(head.x - DOT * 1.6, axis + DOT),
                ],
                line,
                egui::Stroke::NONE,
            ));
        } else {
            painter.circle_filled(egui::pos2(at, axis), DOT * 0.6, line);
        }

        let anchor = if last { rect.right() - DOT } else { at };
        let mut rows = Vec::new();
        if !mark.time.is_empty() {
            rows.push(painter.layout_no_wrap(mark.time.clone(), small.clone(), text));
        }
        // The names of one place stand one block under another, a line of
        // nothing between them, because two names run together read as one.
        rows.push(painter.layout_no_wrap(mark.names.join("\n\n"), small.clone(), line));
        let width = rows
            .iter()
            .map(|galley| galley.rect.width())
            .fold(0.0_f32, f32::max);
        let left = (anchor - width / 2.0)
            .max(rect.left())
            .min(rect.right() - width);
        if left < taken {
            continue;
        }
        taken = left + width + ROW_GAP;

        let mut top = bar.bottom() + ROW_GAP;
        for galley in rows {
            let height = galley.rect.height();
            painter.galley(egui::pos2(left, top), galley, text);
            top += height;
        }
    }
}

/// Where each stretch begins and ends across the track, one edge more than
/// there are stretches.
///
/// Every stretch that is over takes the same quarter of the track, whatever it
/// lasted, and the silence since the data stopped takes what is left. The
/// track is not a scale and never was: a wait of fifty milliseconds beside a
/// silence of five minutes is a six-thousandth of it drawn to scale, which is
/// no stretch at all, and the one thing the picture must never say is that
/// something did not happen. What it says instead is the order of the moments
/// and which stretch is which, and how long each of them was is written over
/// it in a number that is the true span.
///
/// The silence keeps at least a quarter with them, because it is the one
/// stretch with no end and the one the plate is left standing for.
fn stretch_edges(stretches: usize, from: f32, to: f32) -> Vec<f32> {
    let inner = (to - from).max(0.0);
    let Some(over) = stretches.checked_sub(1).filter(|over| *over > 0) else {
        return vec![from, to];
    };

    let width = inner * STRETCH_SHARE.min(1.0 / (over + 1) as f32);
    let mut edges = vec![from];
    for _ in 0..over {
        let last = *edges.last().unwrap_or(&from);
        edges.push(last + width);
    }
    edges.push(to);
    edges
}

/// How much of the track one stretch that is over takes.
const STRETCH_SHARE: f32 = 0.25;

/// The times the plate carries, a row each, in the order they happened: the
/// input stopped, the data began, the data ended. Each row says when it was on
/// the clock of this machine and how long ago that is, because the two answer
/// two questions — which moment of the evening it was, and how far back it is
/// from now — and a plate read while a line goes quiet is opened for the
/// second of them.
///
/// A moment nothing ever happened at is a dash rather than a missing row: the
/// row says which question is unanswered, and a plate that changes its own
/// shape is a plate that has to be read again every time. It has no reading of
/// how long ago, because nothing is what it would be counting from.
///
/// Data that began and ended at the same moment says how long ago it was once,
/// on the row of its end: one chunk of bytes is one moment, and the same number
/// twice reads as two.
fn data_rows(
    data: Option<crate::session::Moment>,
    first: Option<crate::session::Moment>,
    input: Option<crate::session::Moment>,
    answered: u64,
) -> Vec<(String, String, String)> {
    let never = t!("status.data_never").to_string();
    let row = |key: &str, moment: Option<crate::session::Moment>, ago: bool| {
        let (at, since) = match moment {
            Some(moment) if ago => (
                crate::format::moment(moment.at),
                t!(
                    "status.data_ago",
                    span = crate::format::duration(moment.elapsed())
                )
                .to_string(),
            ),
            Some(moment) => (crate::format::moment(moment.at), String::new()),
            None => (never.clone(), String::new()),
        };
        (t!(key).to_string(), at, since)
    };

    let once = match (first, data) {
        (Some(first), Some(data)) => first.since == data.since,
        _ => false,
    };

    // The rows run the way the thing they describe ran: the input stopped, the
    // data began, the data ended. The spans between those moments are written
    // over the track above them, where each one stands on the stretch it
    // measures. Last comes how much that stretch carried, which is the one row
    // that is a size and not a moment, and the row the other three are read for.
    vec![
        row("status.data_input_at", input, true),
        row("status.data_first_at", first, !once),
        row("status.data_at", data, true),
        (
            t!("status.data_answered").to_string(),
            crate::format::volume(answered),
            String::new(),
        ),
    ]
}

/// The sign that a program of the session asked for the mouse, right of the
/// sign of what kind of guest it is.
///
/// It stands there only while a program is asking, because a pointer that
/// behaves the way a pointer behaves needs no sign. Green is the mouse the
/// program has; red is the mouse this window took back, and then the program is
/// left unanswered while the pointer selects, scrolls and opens the menu again.
/// Pressing it turns it over, and so does holding `ctrl+shift` for as long as
/// it is held — the sign turns over with it, because it says what the pointer
/// is doing and not what the switch behind it stands on.
fn mouse_grab(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    if !app.session.terminal.modes().mouse_report {
        return;
    }

    let answered = app.mouse_reports(context);
    let (color, hint) = if answered {
        (
            egui::Color32::from_rgb(0x5c, 0xb8, 0x5c),
            t!("status.mouse_grabbed"),
        )
    } else {
        (ui.visuals().error_fg_color, t!("status.mouse_kept"))
    };

    let clicked = ui
        .add(egui::Button::new(
            egui::RichText::new(icons::MOUSE).color(color),
        ))
        .on_hover_text(hint)
        .clicked();
    if clicked {
        app.ui.mouse_reports = !app.ui.mouse_reports;
    }
}

/// The sign that a program of this side holds the stream of the session, in the
/// block of signs right of the name of the source.
///
/// While a transfer runs the device is not read into the terminal and the
/// keyboard reaches nothing: both ends of the line belong to that program. It
/// stands with the kind of the session and the mouse of the program, because
/// the three of them say what is being done to the line and none of them is a
/// control that acts on it.
fn captured_sign(app: &App, ui: &mut egui::Ui) {
    if !app.session.is_transferring() {
        return;
    }

    ui.label(egui::RichText::new(icons::CAPTURED).color(ui.visuals().warn_fg_color))
        .on_hover_text(t!("status.captured_hint"));
}

/// What kind of guest the session is, right of the connection button, and the
/// switch that says so.
///
/// Pressing it turns the judgement over and writes it down, because what a
/// console carries is known while it runs — an `ssh` was opened, a log is being
/// followed — and the settings are the wrong place to walk to for it. A session
/// on a line cannot be pressed: a device is never trusted, and the sign says
/// that rather than offering a switch that would decide nothing.
fn session_kind(app: &mut App, ui: &mut egui::Ui) {
    if !app.session.has_source() {
        return;
    }

    let console = app.session.console_id().is_some();
    let trusted = app.session_is_trusted();
    let icon = if trusted {
        icons::TRUSTED
    } else {
        icons::UNTRUSTED
    };
    let color = icons::trust_color(ui, trusted);

    ui.label(egui::RichText::new("|").weak());
    let response = ui.add_enabled(
        console,
        egui::Button::new(egui::RichText::new(icon).color(color)),
    );

    // The sign carries no hint of its own. One name — "trusted output" — said
    // less than the pointer resting there deserves, and spelling out what trust
    // decides takes a column of the settings page. So the plate says it: the
    // sequences this session may ask for, by the names that page uses.
    //
    // The right button is what leaves it standing, because the left one is the
    // switch. It is held down until the pointer leaves for the reason the plate
    // of the times is: the press that unpins lands on the sign the pointer is
    // resting on, which would raise it again on the same frame.
    if response.secondary_clicked() {
        app.ui.trust_plate_pinned = !app.ui.trust_plate_pinned;
        app.ui.trust_plate_hidden = !app.ui.trust_plate_pinned;
    }
    if !response.hovered() {
        app.ui.trust_plate_hidden = false;
    }

    let standing = app.ui.trust_plate_pinned || (response.hovered() && !app.ui.trust_plate_hidden);
    if standing && trust_plate(app, ui, response.rect) {
        app.ui.trust_plate_pinned = false;
    }
    if app.ui.trust_plate_pinned && clicked_beside(ui, app.ui.trust_plate_rect, response.rect) {
        app.ui.trust_plate_pinned = false;
    }

    if response.clicked() {
        app.toggle_session_trusted();
    }

    selection_sign(app, ui);
}

/// The sign that a selection is being picked out, beside the sign of trust.
///
/// It stands only while one is, and it is pressed to let it go: a mode the
/// window is in has to say so somewhere that is not the output, and the one
/// place a person already looks for what the session is doing is this row.
///
/// Red, and the red of the theme rather than a red of its own, because what it
/// says is that the keys and the pointer are not the device's for as long as it
/// stands.
fn selection_sign(app: &mut App, ui: &mut egui::Ui) {
    if !app.session.terminal.selecting() {
        return;
    }

    let clicked = ui
        .add(egui::Button::new(
            egui::RichText::new(icons::SELECTION).color(ui.visuals().error_fg_color),
        ))
        .on_hover_text(t!("status.selection"))
        .clicked();
    if clicked {
        app.leave_selection();
    }
}

/// The plate of the sign of trust: what this session may ask for.
///
/// One row per sequence that is honoured, named and numbered the way the
/// settings page names and numbers it, and nothing else — no switch, because
/// nothing here is set, and no row for what is refused, because the question
/// the sign raises is what a guest *may* do. A session that may ask for nothing
/// says so in one line.
///
/// Answers true when it was pressed, which is what takes it down.
fn trust_plate(app: &mut App, ui: &mut egui::Ui, sign: egui::Rect) -> bool {
    let osc = *app.osc();
    let clipboard =
        Some(osc.clipboard).filter(|setting| *setting != crate::config::ClipboardSetting::Disabled);
    let allowed: Vec<crate::config::OscSequence> = crate::config::OscSequence::ALL
        .iter()
        .copied()
        .filter(|sequence| osc.allows(*sequence))
        .collect();

    let above = egui::pos2(sign.left(), ui.max_rect().top() - PLATE_GAP);
    let plate = egui::Area::new(ui.id().with("trust_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(above)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.label(
                    egui::RichText::new(crate::ui::settings::trust_hint(app.session_is_trusted()))
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(t!("settings.osc_allowed_here"))
                        .weak()
                        .small(),
                );
                if clipboard.is_none() && allowed.is_empty() {
                    ui.label(t!("settings.osc_denied"));
                    return;
                }
                ui.separator();
                egui::Grid::new("trust_plate_rows")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        if let Some(setting) = clipboard {
                            // The clipboard is four settings rather than a
                            // switch, so the row says which of them stands: a
                            // row that read "clipboard" would leave the one
                            // question this plate is asked unanswered.
                            plate_row(ui, setting.label_key(), "OSC-52");
                        }
                        for sequence in allowed {
                            plate_row(ui, sequence.label_key(), sequence.code());
                        }
                    });
            });
        });

    app.ui.trust_plate_rect = plate.response.rect;
    plate
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// One row of that plate: the name of a sequence and the numbers it answers.
fn plate_row(ui: &mut egui::Ui, label: &str, code: &str) {
    ui.label(t!(label));
    ui.label(egui::RichText::new(code).weak().small());
    ui.end_row();
}

fn line_params(app: &mut App, ui: &mut egui::Ui) {
    let summary = app.session.params.summary();
    crate::ui::choice::row(ui, app, crate::ui::choice::Choice::LineParams, &summary);
}

/// The lines of a port, with the button of the plate at the head of them.
///
/// The letters the device drives carry no hint. What a hint said — the name of
/// the line written out and whether it is up — is what the plate says, and it
/// says it for all of them at once and over time rather than one at a time and
/// only for now; the button that raises it is [`signals_button`], and it stands
/// first for that reason. The two lines this side drives do carry a hint, because
/// what their two buttons do is the one thing here no picture answers.
///
/// Which of them stand is the device's own answer (`ShownLines`), and the plate
/// draws exactly the same set: the row and the tracks are the same signals read
/// two ways, so a line switched off is off in both. A row where a device shows
/// nothing is an empty row, and the plate is then a plate of the data alone —
/// which is a device that was asked for that.
fn modem_lines(app: &mut App, ui: &mut egui::Ui) {
    use crate::config::StatusLine;

    let lines = app.session.lines;
    let shows = app.shown_lines();

    signals_button(app, ui);

    if shows.shows(StatusLine::Break) {
        break_switch(app, ui);
    }
    if shows.shows(StatusLine::Hold) {
        hold_switch(app, ui);
    }

    if shows.shows(StatusLine::Rts) {
        driven_line(app, ui, false, lines.rts_up());
    }
    if shows.shows(StatusLine::Dtr) {
        driven_line(app, ui, true, lines.dtr_up());
    }

    for (line, level) in [
        (StatusLine::Cts, lines.cts),
        (StatusLine::Dsr, lines.dsr),
        (StatusLine::Carrier, lines.cd),
        (StatusLine::Ring, lines.ri),
    ] {
        if shows.shows(line) {
            line_label(ui, line.label(), level);
        }
    }

    let shown = crate::ui::choice::flow_label(app.session.params.flow_control);
    crate::ui::choice::row(ui, app, crate::ui::choice::Choice::FlowControl, &shown);
}

/// The button the plate of the signals hangs from, at the head of the line
/// controls.
///
/// One control and not a row of them. Every letter beside it says what one line
/// is doing now and is worked with both of its buttons — the break and the hold
/// are turned over, the two lines this side drives are held and given a
/// direction, the flow control opens its list — so a plate that rose from any of
/// them rose while somebody was doing something else. This one does nothing but
/// show it, and it shows it for the whole of the source at once.
///
/// It stands for every source, a console as well as a port: a console has no
/// lines, and how much it said and when is the same question as `RX` on a port.
/// So the button is in the same place whatever is connected, rather than
/// appearing with a device.
///
/// The pointer raises the plate and either button leaves it standing. There is
/// nothing else the button could do with a press, and a picture read while both
/// hands are typing cannot be a picture held up by a pointer standing still. A
/// press lands on the button the pointer is resting on, which is what would raise
/// the plate again on the same frame, so it is held down until the pointer leaves
/// — pressed twice, the button takes the plate down and keeps it down.
fn signals_button(app: &mut App, ui: &mut egui::Ui) {
    let response = ui
        .add(egui::Button::new(icons::SIGNALS).selected(app.ui.signals_pinned))
        .on_hover_text(t!("command.view.toggle_signals"));

    if response.clicked() || response.secondary_clicked() {
        app.ui.signals_pinned = !app.ui.signals_pinned;
        app.ui.signals_hidden = !app.ui.signals_pinned;
    }
    if !response.hovered() {
        app.ui.signals_hidden = false;
    }
    app.ui.signals_hovered = response.hovered();
}

/// One of the two lines this side drives: a switch, and the direction it is
/// switched in.
///
/// Who drives the line and what the line is doing are two answers and not one:
/// opening a port raises both of these lines before anything has asked for
/// anything, and a control that showed only what was asked would call them down
/// while they stand up. So the shape says the first — it stands pressed while the
/// line is held from here rather than left to the driver — and the colour says
/// the second, green for a line that is up and grey for one that is down.
///
/// The left button holds the line and hands it back, and the right button says
/// which way a hold takes it. A switch and not a menu of three states, because
/// the two questions are not asked as often as each other: which level holds a
/// board in reset is a fact about that board, said once and written down for it,
/// and the holding is then one press.
fn driven_line(app: &mut App, ui: &mut egui::Ui, dtr: bool, up: bool) {
    use crate::config::StatusLine;

    let hold = match dtr {
        true => app.session.dtr_hold,
        false => app.session.rts_hold,
    };
    let name = StatusLine::driven(dtr).label();
    let force = app.line_force(dtr);
    let state = match crate::config::LineForce::of_hold(hold) {
        Some(held) => t!(crate::ui::choice::force_hint_key(held)),
        None => t!("force.free_hint"),
    };
    let press = t!("force.press", force = crate::ui::choice::force_label(force));

    let response = ui
        .selectable_label(
            hold.is_forced(),
            egui::RichText::new(name).color(level_color(ui, up)),
        )
        .on_hover_text(format!("{name}: {state}\n{press}"));

    if response.clicked() {
        app.toggle_line_hold(dtr);
    }
    if response.secondary_clicked() {
        crate::ui::choice::open(app, crate::ui::choice::Choice::LineForce { dtr });
    }
}

/// The switch that stops the port being read.
///
/// It stands beside the break because the two are the same kind of thing: both
/// are a state this side holds until it lets go, and both stop the line
/// carrying. The break stops it in one direction and this stops it in the
/// other.
///
/// Where the bytes gather while it is held is what the source is, and the hint
/// says which of the three it is. A port with flow control gathers them in the
/// driver and tells the device to wait, so nothing is lost and the plate shows
/// `RTS` falling for exactly as long. A port without it has no way of telling the
/// device anything, so what the driver cannot hold is lost — the same loss such a
/// line always has, asked for on purpose. A console gathers them in the pipe of
/// its pseudo terminal, and the program waits at its next write.
///
/// It stands for a console as well as a port, because the reason for it is the
/// same for both: a program pouring out text somebody wants to read a page of.
fn hold_switch(app: &mut App, ui: &mut egui::Ui) {
    let held = app.session.read_hold;
    let color = match held {
        true => ui.visuals().warn_fg_color,
        false => ui.visuals().weak_text_color(),
    };
    let hint = match (app.session.is_serial(), app.session.params.flow_control) {
        (false, _) => t!("hold.console"),
        (true, zyt_serial::FlowControl::None) => t!("hold.without_flow"),
        (true, _) => t!("hold.with_flow"),
    };
    let response = ui
        .selectable_label(held, egui::RichText::new("HOLD").color(color))
        .on_hover_text(format!("{}: {hint}", t!("hold.name")));
    if response.clicked() {
        app.toggle_read_hold();
    }
}

/// The switch that holds the transmission line in the break condition.
///
/// It stands with the modem lines because it is one more thing this side does to
/// the line, and ahead of all of them because of what it does: a held break is a
/// line that carries no byte at all, which is the one thing here that explains
/// every other reading beside it going quiet. The lines after it say what a level
/// is; this one says whether there is a line.
///
/// It is a switch and not a button because a break is a state: the line is held
/// there until it is let go, which is what a device reading it as a request for
/// attention waits for. A pressed switch is a line that cannot carry a byte, so
/// it wears the colour the window warns in.
fn break_switch(app: &mut App, ui: &mut egui::Ui) {
    let held = app.session.held_break;
    let color = match held {
        true => ui.visuals().warn_fg_color,
        false => ui.visuals().weak_text_color(),
    };
    let hint = match held {
        true => t!("line.break_held"),
        false => t!("line.break_free"),
    };
    let response = ui
        .selectable_label(held, egui::RichText::new("BRK").color(color))
        .on_hover_text(format!("{}: {hint}", t!("line.break")));
    if response.clicked() {
        app.toggle_break();
    }
}

/// The way to the panel of everything that is running, left of the gear.
///
/// One button and not a row of them: what runs is a list, the list is the
/// panel, and a bar that grew a control per running thing would move every
/// control beside it every time one started.
///
/// The pointer resting on it shows that panel to be read, a press leaves it
/// standing with its buttons, and nothing is drawn at all while nothing runs.
fn tasks_control(app: &mut App, ui: &mut egui::Ui) {
    let jobs = app.tasks.len() + app.jobs.jobs().len() + usize::from(app.session.is_transferring());
    if jobs == 0 {
        app.ui.tasks_hovered = false;
        app.ui.tasks_button_rect = egui::Rect::NOTHING;
        return;
    }

    let response = ui
        .add(egui::Button::new(format!("{} {jobs}", icons::TASKS)).selected(app.show_tasks))
        .on_hover_text(t!("tasks.open_hint"));
    app.ui.tasks_hovered = response.hovered();
    app.ui.tasks_button_rect = response.rect;
    if response.clicked() {
        app.show_tasks = !app.show_tasks;
    }
}

/// How far along the program says it is (OSC 9;4), left of the buttons.
///
/// It is a bar of its own and not a word on the transfer button, because the
/// two are not the same thing: any program of the session may say how far it
/// has got — a build, a flash, a copy — and most of them run with no transfer
/// anywhere, so a share shown on a button only appeared when something else
/// happened to be running. A bar also says at a glance what a number has to be
/// read to say.
///
/// Nothing is drawn while nothing is reported, so the bar takes no room in a
/// window where no program ever speaks.
fn progress_bar(app: &App, ui: &mut egui::Ui) {
    use zyt_term::ProgressState;

    let Some(state) = app.session.progress() else {
        return;
    };

    let (share, color, hint) = match state {
        ProgressState::Removed => return,
        ProgressState::Set(share) => (
            Some(share),
            ui.visuals().selection.bg_fill,
            t!("status.progress"),
        ),
        ProgressState::Error(share) => (
            Some(share),
            ui.visuals().error_fg_color,
            t!("status.progress_error"),
        ),
        ProgressState::Paused(share) => (
            Some(share),
            ui.visuals().warn_fg_color,
            t!("status.progress_paused"),
        ),
        ProgressState::Indeterminate => (
            None,
            ui.visuals().selection.bg_fill,
            t!("status.progress_unknown"),
        ),
    };

    let bar = egui::ProgressBar::new(share.map_or(1.0, |share| f32::from(share) / 100.0))
        .desired_width(PROGRESS_WIDTH * height_scale(ui))
        .fill(color)
        .animate(share.is_none());
    let bar = match share {
        Some(share) => bar.text(format!("{share}%")),
        None => bar,
    };
    ui.add(bar).on_hover_text(hint);
}

/// How wide that bar stands at the height the toolkit draws at. A share is read
/// from the length of the bar and the number written in it, and neither wants
/// more room than this.
const PROGRESS_WIDTH: f32 = 96.0;

/// How much taller than usual this bar was asked to be.
///
/// The height of the status bar is set by growing the style it is drawn with,
/// so the style itself says the factor: a bar of progress is the one thing here
/// with a width of its own, and it grows with everything else rather than
/// staying a stripe in a tall bar.
fn height_scale(ui: &egui::Ui) -> f32 {
    ui.spacing().interact_size.y / egui::style::Spacing::default().interact_size.y
}

/// Bytes that have not left the port yet, and the way to give up on them.
///
/// It is a button and not a word because the number is read at the one moment
/// somebody wants to act on it: a paste nobody meant to make, on a line too slow
/// to carry it, is minutes of a window that answers nothing. What has reached the
/// line is gone, so what the button throws away is what has not.
///
/// It stands only while there is something to throw away. A button that is there
/// while the count is nought would be a button that does nothing, and the count is
/// the whole of what it is for.
fn line_state(app: &mut App, ui: &mut egui::Ui) {
    let pending = app.session.pending_output();
    if pending == 0 {
        return;
    }
    let clicked = ui
        .add(egui::Button::new(
            egui::RichText::new(format!("TX {pending}")).color(ui.visuals().warn_fg_color),
        ))
        .on_hover_text(t!("status.line_discard", count = pending))
        .clicked();
    if clicked {
        app.discard_line_output();
    }
}

/// How long the transfer runs, or how long the last one took.
///
/// Which profile it uses is not asked here any more: a transfer is started
/// from the menu of the terminal or from the menu of a dropped file, and the
/// profile is chosen there, beside the direction it belongs to.
fn transfer_time(app: &App, ui: &mut egui::Ui) {
    let (span, hint) = match app.session.transfer_elapsed() {
        Some(elapsed) => (elapsed, t!("status.transfer_elapsed")),
        None => match app.session.last_transfer() {
            Some(last) => (last, t!("status.transfer_last")),
            None => return,
        },
    };

    ui.label(egui::RichText::new(crate::format::duration(span)).weak())
        .on_hover_text(hint);
}

/// Indicator of a line the peer drives.
fn line_label(ui: &mut egui::Ui, name: &str, level: bool) {
    ui.colored_label(level_color(ui, level), name);
}

pub(super) fn level_color(ui: &egui::Ui, level: bool) -> egui::Color32 {
    if level {
        egui::Color32::from_rgb(0x5c, 0xb8, 0x5c)
    } else {
        ui.visuals().weak_text_color()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Moment;
    use std::time::{Duration, Instant};

    /// A moment that many seconds ago.
    fn ago(seconds: u64) -> Moment {
        let span = Duration::from_secs(seconds);
        Moment {
            at: jiff::Timestamp::now() - jiff::SignedDuration::from_secs(seconds as i64),
            since: Instant::now()
                .checked_sub(span)
                .expect("the clock of this machine has been running that long"),
        }
    }

    /// Every stretch that is over takes the same quarter of the track, whatever
    /// it lasted: the track says what happened in which order, and how long
    /// each of them took is the number written over it.
    #[test]
    fn every_stretch_that_is_over_takes_a_quarter_of_the_track() {
        let edges = stretch_edges(3, 0.0, 100.0);

        assert_eq!(edges.len(), 4, "one edge more than there are stretches");
        assert!((edges[1] - edges[0] - 25.0).abs() < 0.01, "{edges:?}");
        assert!((edges[2] - edges[1] - 25.0).abs() < 0.01, "{edges:?}");
    }

    /// The same quarter however long a stretch lasted, and the silence takes
    /// what is left: it is the one stretch with no end, and the one the plate
    /// is left standing for.
    #[test]
    fn the_silence_takes_what_is_left_of_the_track() {
        let edges = stretch_edges(3, 0.0, 100.0);
        assert!((edges[3] - edges[2] - 50.0).abs() < 0.01, "{edges:?}");

        let one = stretch_edges(2, 0.0, 100.0);
        assert!((one[2] - one[1] - 75.0).abs() < 0.01, "{one:?}");

        let alone = stretch_edges(1, 0.0, 100.0);
        assert_eq!(
            alone,
            vec![0.0, 100.0],
            "a track of one stretch is all of it"
        );
        assert_eq!(stretch_edges(0, 0.0, 100.0), vec![0.0, 100.0]);
    }

    /// Moments that fell together are one place on the track, so a track of
    /// two of them is two stretches and not three.
    #[test]
    fn the_stretches_of_the_track_are_the_places_on_it() {
        let moment = ago(4);
        let stacked = track_marks(ago(1), moment, Some(moment));
        let apart = track_marks(ago(1), ago(3), Some(ago(6)));

        assert_eq!(stacked.len(), 2);
        assert_eq!(apart.len(), 3);
    }

    /// What each mark of the track is called, in the order they stand.
    fn names(marks: &[Mark]) -> Vec<Vec<String>> {
        marks.iter().map(|mark| mark.names.clone()).collect()
    }

    /// Three moments that are three places are three dots, each with the one
    /// name of what happened there.
    #[test]
    fn three_moments_of_their_own_stand_on_three_dots() {
        let marks = track_marks(ago(1), ago(3), Some(ago(6)));

        assert_eq!(
            names(&marks),
            [
                vec![t!("status.track_input").to_string()],
                vec![t!("status.track_first").to_string()],
                vec![t!("status.track_last").to_string()],
            ]
        );
        assert!(
            !marks[0].began,
            "the wait before the answer leaves the first"
        );
        assert!(marks[1].began, "and the answer itself leaves the second");
    }

    /// A write answered in no time at all and the byte that answered it are one
    /// place on the line, so they are one dot with both names under it.
    #[test]
    fn a_write_answered_at_once_stands_with_the_answer() {
        let moment = ago(4);
        let marks = track_marks(ago(1), moment, Some(moment));

        assert_eq!(
            names(&marks),
            [
                vec![
                    t!("status.track_input").to_string(),
                    t!("status.track_first").to_string(),
                ],
                vec![t!("status.track_last").to_string()],
            ]
        );
        assert!(
            marks[0].began,
            "the data had begun there, so what leaves it is the answer"
        );
    }

    /// Data that began and ended at one moment is named once and by its end: a
    /// dot saying it began and ended there says twice what happened once.
    #[test]
    fn data_of_one_moment_is_named_by_its_end() {
        let moment = ago(2);
        let marks = track_marks(moment, moment, Some(ago(5)));

        assert_eq!(
            names(&marks),
            [
                vec![t!("status.track_input").to_string()],
                vec![t!("status.track_last").to_string()],
            ]
        );
    }

    /// One chunk of bytes back at the moment of the write leaves one place, and
    /// the track is the silence since it.
    #[test]
    fn a_line_of_one_moment_stands_on_one_dot() {
        let moment = ago(3);
        let marks = track_marks(moment, moment, Some(moment));

        assert_eq!(
            names(&marks),
            [vec![
                t!("status.track_input").to_string(),
                t!("status.track_last").to_string(),
            ]]
        );
    }

    /// A write that came after the data it stands beside answered nothing, so
    /// the track begins at the data: what it would draw is a stretch running
    /// backwards.
    #[test]
    fn a_write_after_the_data_is_left_off_the_track() {
        let marks = track_marks(ago(4), ago(6), Some(ago(2)));

        assert_eq!(
            names(&marks),
            [
                vec![t!("status.track_first").to_string()],
                vec![t!("status.track_last").to_string()],
            ]
        );
    }

    /// The plate makes room for the tallest of the labels, and a name of two
    /// lines beside another is a line of nothing between them.
    #[test]
    fn the_rows_of_a_mark_are_its_names_and_the_clock_over_them() {
        let moment = ago(4);
        let stacked = track_marks(ago(1), moment, Some(moment));
        let alone = track_marks(ago(1), ago(3), Some(ago(6)));

        assert_eq!(mark_rows(&alone[0]), 3, "the clock and a name of two lines");
        assert_eq!(
            mark_rows(&stacked[0]),
            6,
            "the clock, two names of two lines, and the line of nothing between"
        );
    }

    /// How long ago the row this key names says it was.
    fn since(rows: &[(String, String, String)], key: &str) -> String {
        rows.iter()
            .find(|(name, _, _)| name.as_str() == t!(key))
            .unwrap_or_else(|| panic!("{key} stands on a row of its own"))
            .2
            .clone()
    }

    /// The value of the row of that name, which is its middle column.
    fn at(rows: &[(String, String, String)], key: &str) -> String {
        rows.iter()
            .find(|(name, _, _)| name.as_str() == t!(key))
            .unwrap_or_else(|| panic!("{key} stands on a row of its own"))
            .1
            .clone()
    }

    /// The rows run the way the thing they describe ran, so the plate is read
    /// down the way the line was: the input stopped, the data began, the data
    /// ended.
    #[test]
    fn the_rows_run_in_the_order_they_happened() {
        let rows = data_rows(Some(ago(2)), Some(ago(4)), Some(ago(5)), 0);

        let names: Vec<&str> = rows.iter().map(|(name, _, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                t!("status.data_input_at"),
                t!("status.data_first_at"),
                t!("status.data_at"),
                t!("status.data_answered"),
            ],
            "and how much it carried last, which is what the three are read for"
        );
    }

    /// Every row says how long ago its moment was, because a plate opened
    /// while a line goes quiet is opened for that reading.
    #[test]
    fn every_row_says_how_long_ago_its_moment_was() {
        let rows = data_rows(Some(ago(7)), Some(ago(9)), Some(ago(11)), 0);

        assert!(since(&rows, "status.data_input_at").starts_with("11"));
        assert!(since(&rows, "status.data_first_at").starts_with('9'));
        assert!(since(&rows, "status.data_at").starts_with('7'));
    }

    /// How much the answer carried is counted in bytes while it is short and in
    /// kibibytes once it is not.
    ///
    /// A short answer is what somebody counts byte by byte — thirty-seven bytes
    /// are thirty-seven bytes and not nought point nought kibibytes — and past a
    /// hundred kibibytes the last three digits are noise.
    #[test]
    fn the_answer_is_counted_in_the_units_it_is_read_in() {
        let bytes = data_rows(Some(ago(1)), Some(ago(2)), Some(ago(3)), 37);
        assert_eq!(
            at(&bytes, "status.data_answered"),
            crate::format::volume(37)
        );
        assert!(at(&bytes, "status.data_answered").starts_with("37"));

        let large = crate::format::VOLUME_STEP;
        let kibibytes = data_rows(Some(ago(1)), Some(ago(2)), Some(ago(3)), large);
        assert_eq!(
            at(&kibibytes, "status.data_answered"),
            crate::format::volume(large)
        );
        assert!(at(&kibibytes, "status.data_answered").starts_with("100"));
    }

    /// Data that began and ended at one moment says how long ago that was
    /// once: the same number twice reads as two.
    #[test]
    fn data_of_one_moment_says_how_long_ago_it_was_once() {
        let moment = ago(7);
        let rows = data_rows(Some(moment), Some(moment), Some(ago(9)), 0);

        assert_eq!(since(&rows, "status.data_first_at"), "");
        assert!(since(&rows, "status.data_at").starts_with('7'));
        assert_eq!(
            rows[1].1, rows[2].1,
            "and the moment itself stands on both rows"
        );
    }

    /// The plate asks for a frame once in ten seconds and never oftener: the
    /// one number that counts from now is read by somebody watching a line go
    /// quiet, and what it is to a tenth is not what they came for.
    ///
    /// The wait is to the next step and not a step long, so the steps fall on
    /// the same moments whenever the plate is opened.
    #[test]
    fn the_plate_asks_for_a_frame_once_in_ten_seconds() {
        assert_eq!(next_tick(Duration::ZERO), PLATE_TICK);
        assert_eq!(
            next_tick(Duration::from_millis(150)),
            Duration::from_millis(9_850)
        );
        assert_eq!(
            next_tick(Duration::from_millis(59_900)),
            Duration::from_millis(100),
            "to the next step, wherever the plate was opened"
        );
        assert_eq!(next_tick(Duration::from_secs(60)), PLATE_TICK);

        for span in [0, 150, 59_900, 60_000, 61_250, 3_600_000] {
            assert!(
                next_tick(Duration::from_millis(span)) <= PLATE_TICK,
                "a frame oftener than that, at {span} ms"
            );
        }
    }

    /// Nothing came and nothing was sent, and every row says so rather than
    /// going missing.
    ///
    /// The three moments say they never happened. The size says nought, because
    /// nought bytes is what an answer that never came carried and a size has no
    /// "never" to say.
    #[test]
    fn a_line_that_said_nothing_is_named_and_not_left_out() {
        let rows = data_rows(None, None, None, 0);
        let never = t!("status.data_never").to_string();
        let (moments, sizes) = rows.split_at(3);

        assert_eq!(rows.len(), 4);
        assert!(
            moments.iter().all(|(_, at, _)| *at == never),
            "every moment of them is unanswered: {moments:?}"
        );
        assert_eq!(
            sizes
                .iter()
                .map(|(_, at, _)| at.as_str())
                .collect::<Vec<_>>(),
            [crate::format::volume(0).as_str()]
        );
        assert!(
            rows.iter().all(|(_, _, ago)| ago.is_empty()),
            "and none of them counts from a moment that never was: {rows:?}"
        );
    }
}
