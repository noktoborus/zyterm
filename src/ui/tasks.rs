//! Panel of everything this window has running.

use crate::app::App;
use crate::ui::icons;
use rust_i18n::t;
use zyt_files::{TaskKind, TaskState};
use zyt_script::{JobId, JobState, Outcome};

/// How much of what a job runs stands in its row.
///
/// A command line is as long as it wants to be — a path, a list of paths, a
/// pipeline — and a panel as wide as the longest of them is a panel that moves
/// every time one starts. What is cut off is on the pointer.
const TITLE_LIMIT: usize = 64;

/// Draws the panel when it is wanted, in the middle of the window.
///
/// One line per thing that runs: what it does, how long it has been going, and
/// the buttons that act on it. The buttons are there while the panel stands on
/// its own and not while it is only being looked at from the button that opens
/// it: a row under a pointer that came to read is a row nothing should be
/// pressed on by accident.
///
/// It is a window with a title and a cross and it does not move: a list of what
/// is running is read where it was last read, and a panel dragged out from
/// under the eye is a panel looked for the next time. The cross is the one the
/// toolkit draws in the bar of a window, so it is where every other cross of
/// the desktop is; while the panel is only being looked at there is no bar at
/// all, because nothing of it can be pressed then either.
///
/// There is no button that stops everything. Each row carries what stops the
/// thing it names, and one press that ends a transfer, a copy and a deletion
/// together is a press whose reach is read after it has happened.
pub fn draw(app: &mut App, context: &egui::Context) {
    let running = app.tasks.running();
    let jobs = app.jobs.jobs();
    let transfer = app.session.is_transferring();
    if running.is_empty() && jobs.is_empty() && !transfer {
        app.show_tasks = false;
        return;
    }
    if !app.show_tasks && !app.ui.tasks_hovered {
        return;
    }

    let acts = app.show_tasks;
    let mut cancel = None;
    let mut stop_transfer = false;
    let mut act = None;
    let mut open = true;

    let mut window = egui::Window::new(t!("tasks.title"))
        .id(egui::Id::new("tasks"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .title_bar(acts);
    if acts {
        window = window.open(&mut open);
    }

    let panel = window.show(context, |ui| {
        ui.set_min_width(520.0);
        egui::Grid::new("tasks_grid")
            .num_columns(5)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                if transfer {
                    stop_transfer = transfer_row(app, ui, acts);
                }
                for task in &running {
                    task_row(ui, task, acts, &mut cancel);
                }
                for job in &jobs {
                    job_row(ui, job, acts, &mut act);
                }
            });
    });

    if let Some(id) = cancel {
        app.tasks.cancel(id);
    }
    match act {
        Some((id, Act::Cancel)) => app.cancel_job(id),
        Some((id, Act::Forget)) => app.forget_job(id),
        Some((id, Act::Open)) => {
            if let Some(job) = jobs.iter().find(|job| job.id == id) {
                let log = job.log.clone();
                app.open_log(&log);
            }
        }
        None => {}
    }
    if stop_transfer {
        let outcome = app.session.cancel_transfer();
        app.report(outcome);
    }
    if !open || context.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.show_tasks = false;
    }
    let rect = panel.map_or(egui::Rect::NOTHING, |panel| panel.response.rect);
    if app.show_tasks && clicked_beside(app, context, rect) {
        app.show_tasks = false;
    }
}

/// True when this frame carried a press that landed neither on the panel nor
/// on the button that opens it.
///
/// The button is left out because a press on it is the press that closes the
/// panel already, and a panel closed twice in one frame is a panel that opens
/// again on the next one.
fn clicked_beside(app: &App, context: &egui::Context, panel: egui::Rect) -> bool {
    context.input(|input| {
        let Some(at) = input.pointer.interact_pos() else {
            return false;
        };
        input.pointer.any_click() && !panel.contains(at) && !app.ui.tasks_button_rect.contains(at)
    })
}

/// The row of the transfer that holds the line. True when it should be stopped.
///
/// A transfer is not one of the file tasks — it belongs to the session and is
/// stopped through it — but it is one more thing that is running, so it stands
/// in the same list. It leaves no file behind and nothing of it outlives it, so
/// the last two buttons of the row are not its to press.
fn transfer_row(app: &App, ui: &mut egui::Ui, acts: bool) -> bool {
    let name = app
        .session
        .script_name()
        .map(str::to_string)
        .unwrap_or_default();
    ui.label(title(&format!("{} {}", t!("status.transfer"), name)))
        .on_hover_text(t!("status.captured_hint"));

    elapsed(ui, app.session.transfer_elapsed().unwrap_or_default(), None);

    let stop = acts
        && ui
            .button(icons::CLOSE)
            .on_hover_text(t!("status.cancel_transfer_hint"))
            .clicked();
    ui.label("");
    ui.label("");
    ui.end_row();
    stop
}

/// One file operation: it runs until it does not, and then it is gone.
fn task_row(
    ui: &mut egui::Ui,
    task: &TaskState,
    acts: bool,
    cancel: &mut Option<zyt_files::TaskId>,
) {
    let name = task
        .path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| task.path.display().to_string());
    ui.label(title(&format!("{} {name}", t!(label_key(task.kind)))))
        .on_hover_text(task.path.display().to_string());

    elapsed(ui, task.elapsed(), task.remaining());

    let hint = if task.cancelled {
        t!("tasks.stopping")
    } else {
        t!("tasks.cancel_one", name = task.path.display().to_string())
    };
    if acts
        && ui
            .add_enabled(!task.cancelled, egui::Button::new(icons::CLOSE))
            .on_hover_text(hint)
            .clicked()
    {
        *cancel = Some(task.id);
    }
    ui.label("");
    ui.label("");
    ui.end_row();
}

/// What a button of a row of the list does.
enum Act {
    /// Stop the program.
    Cancel,
    /// Take the row away, and the file of its output with it.
    Forget,
    /// Read that file with whatever the desktop opens text with.
    Open,
}

/// One transfer beside the line, running or finished.
///
/// It stays in the list after it ends, because the file it wrote is reached
/// from here and from nowhere else, and the color says how it ended before the
/// file is opened at all.
fn job_row(ui: &mut egui::Ui, job: &JobState, acts: bool, act: &mut Option<(JobId, Act)>) {
    let text = title(&job.title);
    let text = match outcome_color(ui, job) {
        Some(color) => text.color(color),
        None => text,
    };
    ui.label(text).on_hover_text(&job.title);

    elapsed(ui, job.elapsed(), None);

    if !acts {
        ui.label("");
        ui.label("");
        ui.label("");
        ui.end_row();
        return;
    }

    let stoppable = job.is_running() && !job.cancelled;
    if ui
        .add_enabled(stoppable, egui::Button::new(icons::CLOSE))
        .on_hover_text(t!("tasks.cancel_hint"))
        .clicked()
    {
        *act = Some((job.id, Act::Cancel));
    }
    if ui
        .button(icons::REMOVE)
        .on_hover_text(t!("tasks.forget_hint"))
        .clicked()
    {
        *act = Some((job.id, Act::Forget));
    }
    if ui
        .button(icons::LOG)
        .on_hover_text(t!("tasks.log_hint", path = job.log.display().to_string()))
        .clicked()
    {
        *act = Some((job.id, Act::Open));
    }
    ui.end_row();
}

/// The color a finished job wears: green for one that did what it was started
/// for, the color of an error for one that did not, and nothing at all while it
/// is still going.
fn outcome_color(ui: &egui::Ui, job: &JobState) -> Option<egui::Color32> {
    match job.outcome? {
        Outcome::Done => Some(egui::Color32::from_rgb(0x5c, 0xb8, 0x5c)),
        Outcome::Failed(_) | Outcome::Cancelled => Some(ui.visuals().error_fg_color),
    }
}

/// How long it has been going, and how long it has left when that can be
/// guessed.
fn elapsed(ui: &mut egui::Ui, span: std::time::Duration, left: Option<std::time::Duration>) {
    let mut text = crate::format::duration(span);
    if let Some(left) = left {
        text = format!("{text}  {} {}", icons::TO, crate::format::duration(left));
    }
    ui.label(egui::RichText::new(text).weak())
        .on_hover_text(t!("tasks.elapsed"));
}

/// What a row calls the thing it stands for, cut to the width of the column.
fn title(text: &str) -> egui::RichText {
    let flat = text.replace(['\n', '\r'], " ");
    let shown = if flat.chars().count() > TITLE_LIMIT {
        let kept: String = flat.chars().take(TITLE_LIMIT - 1).collect();
        format!("{kept}{}", icons::ELLIPSIS)
    } else {
        flat
    };
    egui::RichText::new(shown).strong()
}

/// Translation key naming what a task does.
pub fn label_key(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Move => "tasks.move",
        TaskKind::Trash => "tasks.trash",
        TaskKind::Delete => "tasks.delete",
        TaskKind::Read => "tasks.read",
        TaskKind::Write => "tasks.write",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_command_is_cut_and_marked() {
        let long = "shell-transfer ".to_string() + &"a".repeat(120);
        let shown = title(&long).text().to_string();

        assert_eq!(shown.chars().count(), TITLE_LIMIT);
        assert!(shown.ends_with(icons::ELLIPSIS));
    }

    #[test]
    fn a_short_command_stands_whole_and_on_one_line() {
        let shown = title("cat 'one'\ncat 'two'").text().to_string();

        assert_eq!(shown, "cat 'one' cat 'two'");
    }
}
