//! The window of numbers: what the program costs while it runs.
//!
//! It floats above everything, is moved and collapsed like any window of the
//! toolkit, and is only there while the settings say so. Nothing in it can be
//! pressed: it is a window to read, opened when somebody is asking where the
//! memory or the processor of this program goes, and the answer is worth having
//! in the running program rather than in a profiler beside it.
//!
//! The numbers of the operating system are read at most twice a second — a
//! number that moves with every frame cannot be read — and everything else is
//! what the program already holds, asked as the window is drawn.
//!
//! It asks for no frames of its own. A window of numbers about the cost of
//! drawing that made the program draw would be measuring itself: every reading
//! would carry the cost of taking it, and an idle terminal watched through this
//! window would never be idle. It is redrawn when the window is redrawn for
//! some other reason, and between two of those its numbers stand where they
//! were — which is what a window that costs nothing to watch looks like.

use crate::app::App;
use crate::metrics::Sample;
use rust_i18n::t;
use std::time::Duration;

/// Share of the height of the window the numbers may take before they are
/// scrolled.
///
/// The window is as tall as its numbers and no taller, until they would not fit
/// the window they float over: a panel of numbers standing from the top edge to
/// the bottom one covers the thing somebody is watching.
const HEIGHT_SHARE: f32 = 0.7;

/// Draws the window while the settings ask for it.
///
/// The cross in its corner takes the setting back down, so the window is closed
/// where it stands and not only from the settings: somebody who opened it to read
/// a number is done with it there.
pub fn draw(app: &mut App, context: &egui::Context) {
    if !app.settings.show_debug_window {
        return;
    }

    let speed = app.session.byte_rate();
    let interval = app.read_interval();
    let sample = app.meter.sample();
    let threads = app.meter.threads().to_vec();
    let frame = app.meter.last_frame();
    let frames = app.meter.frames();

    let mut open = true;
    let height = context.content_rect().height() * HEIGHT_SHARE;
    egui::Window::new(t!("debug.title"))
        .default_pos(egui::Pos2::new(24.0, 24.0))
        .resizable(false)
        .collapsible(true)
        .open(&mut open)
        .show(context, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            crate::ui::widgets::scroll_area(ui)
                .max_height(height)
                .auto_shrink([true, true])
                .show(ui, |ui| {
                    egui::Grid::new("debug_numbers")
                        .num_columns(2)
                        .spacing(egui::Vec2::new(16.0, 2.0))
                        .show(ui, |ui| {
                            process(ui, &sample, &threads);
                            ui.end_row();
                            render(ui, frame, frames);
                            ui.end_row();
                            terminal(ui, app);
                            ui.end_row();
                            buffers(ui, app, speed, interval);
                            ui.end_row();
                            fonts(ui, context);
                        });
                });
        });

    if !open {
        app.settings.show_debug_window = false;
        app.save_settings();
    }
}

/// One line of the window: what it is, and the number.
fn row(ui: &mut egui::Ui, name: &str, value: String) {
    ui.label(name);
    ui.label(egui::RichText::new(value).monospace());
    ui.end_row();
}

/// A heading between two blocks of lines.
fn heading(ui: &mut egui::Ui, name: &str) {
    ui.label(egui::RichText::new(name).strong());
    ui.label("");
    ui.end_row();
}

/// What the answer is when the platform does not report a number.
fn unknown() -> String {
    t!("debug.unknown").to_string()
}

/// Bytes, or the word for a number this platform does not report.
fn bytes(value: Option<u64>) -> String {
    value.map(crate::format::size).unwrap_or_else(unknown)
}

/// What the process costs: the processor it uses and the memory it holds.
fn process(ui: &mut egui::Ui, sample: &Sample, threads: &[(String, usize)]) {
    heading(ui, &t!("debug.process"));
    row(
        ui,
        &t!("debug.processor"),
        sample
            .processor
            .map(|share| format!("{:.0} %", share * 100.0))
            .unwrap_or_else(unknown),
    );
    row(ui, &t!("debug.memory"), bytes(sample.resident));
    row(ui, &t!("debug.memory_peak"), bytes(sample.peak));
    row(ui, &t!("debug.memory_virtual"), bytes(sample.virtual_size));
    ui.label(t!("debug.threads"));
    ui.label(
        egui::RichText::new(
            sample
                .threads
                .map(|count| count.to_string())
                .unwrap_or_else(unknown),
        )
        .monospace(),
    );
    ui.end_row();
    ui.label(named(threads));
    ui.end_row();
}

/// What drawing costs: how long the last frame took this program, and how many
/// frames there were in the last second.
///
/// The first is the time between the start of the work of a pass and the end of
/// it. What the toolkit spends after that — cutting the shapes, handing them to
/// the chip — is not in it.
///
/// The rate is counted and not worked out from the time a frame took. A rate taken from
/// the time one frame took is a rate this program never drew at: a frame of
/// four milliseconds says two hundred and fifty a second of a window that drew
/// one and then waited a minute.
fn render(ui: &mut egui::Ui, frame: Option<std::time::Duration>, frames: usize) {
    heading(ui, &t!("debug.render"));
    row(ui, &t!("debug.frame_time"), milliseconds(frame));
    row(
        ui,
        &t!("debug.frames"),
        format!("{frames} {}", t!("format.per_second")),
    );
}

/// A span in milliseconds, or the word for one there is not.
fn milliseconds(span: Option<std::time::Duration>) -> String {
    span.map(|span| format!("{:.1} ms", span.as_secs_f32() * 1000.0))
        .unwrap_or_else(unknown)
}

/// What the grid holds: the lines above the screen and what they cost.
fn terminal(ui: &mut egui::Ui, app: &App) {
    let (columns, rows) = app.session.terminal.size();
    let history = app.session.terminal.history_size();
    let cap = app.session.terminal.scrollback();
    let line = columns * zyt_term::GRID_CELL_BYTES;

    heading(ui, &t!("debug.terminal"));
    row(ui, &t!("debug.grid"), format!("{columns} \u{d7} {rows}"));
    row(
        ui,
        &t!("debug.history"),
        format!(
            "{history} / {cap} ({})",
            crate::format::size((history * line) as u64)
        ),
    );
    row(
        ui,
        &t!("debug.snapshot"),
        crate::format::size((columns * rows * size_of::<zyt_term::Cell>()) as u64),
    );
}

/// What the buffers hold: the clipboard of this window, the bytes of the line
/// and the ones that came and went — and how long the line took to empty the
/// last time, which is about the line and not about a control, so it stands here
/// rather than in the status bar.
///
/// The speed and the wait stand together because one decides the other: the
/// ladder of the settings is read with the speed of the last second, and what
/// it answered is the wait shown beside it.
fn buffers(ui: &mut egui::Ui, app: &App, speed: f32, interval: Option<Duration>) {
    let (read, read_room) = app.session.read_buffer();

    heading(ui, &t!("debug.buffers"));
    row(ui, &t!("debug.speed"), crate::format::rate(speed));
    row(
        ui,
        &t!("debug.read_interval"),
        match interval {
            Some(wait) => format!("{} {}", wait.as_millis(), t!("format.milliseconds")),
            None => t!("debug.at_once").to_string(),
        },
    );
    row(
        ui,
        &t!("debug.clipboard"),
        crate::format::size(app.stored_clipboard.len() as u64),
    );
    row(
        ui,
        &t!("debug.pending_output"),
        crate::format::size(app.session.pending_output() as u64),
    );
    driver_queues(ui, app);
    row(
        ui,
        &t!("debug.read_buffer"),
        format!(
            "{} / {}",
            crate::format::size(read as u64),
            crate::format::size(read_room as u64)
        ),
    );
    let (waiting, held_back) = app.session.waiting_to_be_read();
    let waiting = crate::format::size(waiting as u64);
    row(
        ui,
        &t!("debug.waiting_to_be_read"),
        match held_back {
            true => format!("{waiting} \u{2014} {}", t!("debug.held_back")),
            false => waiting,
        },
    );
    row(
        ui,
        &t!("debug.bytes_in"),
        crate::format::size(app.session.bytes_in),
    );
    row(
        ui,
        &t!("debug.bytes_out"),
        crate::format::size(app.session.bytes_out),
    );
    row(
        ui,
        &t!("debug.tasks"),
        format!(
            "{} + {}",
            app.tasks.running().len(),
            usize::from(app.session.is_transferring())
        ),
    );
    row(
        ui,
        &t!("debug.line_busy"),
        app.session
            .last_busy()
            .map(crate::format::duration)
            .unwrap_or_else(unknown),
    );
}

/// What the driver of a port holds in each direction.
///
/// It is two numbers and not one, and neither of them is
/// [`crate::session::Session::pending_output`]: that one is everything on this
/// side of the line, the buffer of this program and the queue of the driver
/// together, which is the answer to whether a transfer is over. These two are
/// the queues themselves, which is the answer to where the bytes are standing —
/// a line that is not read fills the first, and a line that cannot carry fills
/// the second.
///
/// A console has neither. A pipe is not a queue anybody can ask the size of, so
/// the rows are not drawn at all rather than drawn as nought.
fn driver_queues(ui: &mut egui::Ui, app: &App) {
    let Some((input, output)) = app.session.driver_queues() else {
        return;
    };
    row(
        ui,
        &t!("debug.driver_input"),
        crate::format::size(input as u64),
    );
    row(
        ui,
        &t!("debug.driver_output"),
        crate::format::size(output as u64),
    );
}

/// What the fonts cost: the files the toolkit holds, the ones it only maps and
/// the atlas it cuts its glyphs from.
///
/// The two are worth telling apart. What is held stands in memory whole and
/// twice, because the toolkit keeps the definitions and clones a blob out of
/// them; what is mapped is a file, of which only the pages that were read stand
/// in memory at all — so that number is the size of the files and not the price
/// of them.
fn fonts(ui: &mut egui::Ui, context: &egui::Context) {
    let (faces, held, mapped, atlas, filled, galleys) = context.fonts_mut(|fonts| {
        let definitions = fonts.definitions();
        let (mut held, mut mapped) = (0usize, 0usize);
        for data in definitions.font_data.values() {
            match data.font {
                std::borrow::Cow::Borrowed(bytes) => mapped += bytes.len(),
                std::borrow::Cow::Owned(ref bytes) => held += bytes.len(),
            }
        }
        (
            definitions.font_data.len(),
            held,
            mapped,
            fonts.font_image_size(),
            fonts.font_atlas_fill_ratio(),
            fonts.num_galleys_in_cache(),
        )
    });

    heading(ui, &t!("debug.fonts"));
    row(ui, &t!("debug.font_faces"), faces.to_string());
    row(ui, &t!("debug.font_held"), crate::format::size(held as u64));
    row(
        ui,
        &t!("debug.font_mapped"),
        crate::format::size(mapped as u64),
    );
    row(
        ui,
        &t!("debug.atlas"),
        format!(
            "{} \u{d7} {} ({}, {:.0} %)",
            atlas[0],
            atlas[1],
            crate::format::size((atlas[0] * atlas[1] * 4) as u64),
            filled * 100.0
        ),
    );
    row(ui, &t!("debug.galleys"), galleys.to_string());
}

/// The threads of the process as one text, a line per name.
///
/// A name says who started the thread: this program names its own, and so do the
/// graphics driver, the clipboard and the bus of the desktop, which is what
/// answers the question of why there are twenty of them.
fn named(threads: &[(String, usize)]) -> String {
    if threads.is_empty() {
        return unknown();
    }
    threads
        .iter()
        .map(|(name, count)| match count {
            1 => name.clone(),
            _ => format!("{name} \u{d7} {count}"),
        })
        .collect::<Vec<String>>()
        .join("\n")
}
