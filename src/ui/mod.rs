//! Interface of the application.

pub mod ask;
mod block;
pub mod choice;
pub mod confirm;
pub mod connect;
mod debug;
mod file_dialog;
pub mod form;
pub mod history;
pub mod icons;
pub mod menu;
pub mod search;
mod selection;
pub mod settings;
mod signals;
mod statusbar;
mod tasks;
mod terminal;
pub mod widgets;

use crate::app::{App, MainView};
use rust_i18n::t;

/// How far a plate over the terminal stands from the edge it rises against.
///
/// One number for every plate of the window — the times, the signals, the sign
/// of trust, a command of several lines — so two of them do not sit at two
/// distances from the same edge.
pub const PLATE_GAP: f32 = 6.0;

/// Asks before a file of the session is thrown away, when the settings say to.
fn confirm_file(app: &mut App, context: &egui::Context) {
    let Some(pending) = app.pending_file.clone() else {
        return;
    };

    let question = t!(pending.question_key());
    let detail = pending.path().display().to_string();
    match confirm::ask(context, "file", question.as_ref(), &detail) {
        confirm::Answer::Yes => {
            app.pending_file = None;
            app.run_pending_file(pending);
        }
        confirm::Answer::No => app.pending_file = None,
        confirm::Answer::Pending => {}
    }
}

/// Takes down a question about a source the window cannot reach that was left
/// without an answer.
///
/// The question is the menu of plates, which is closed by `Esc` — and by a click
/// beside it when the settings say so. A question waved away leads to the picker
/// the way choosing it does, through `App::disconnect`, because it must not be
/// the one that closes the window.
fn lost_source(app: &mut App) {
    if app.lost.is_none() || app.menu.is_open() {
        return;
    }

    app.lost = None;
    app.disconnect();
}

/// Says what a file held over the window will do when it is let go.
fn hovered_files(app: &App, context: &egui::Context) {
    if context.input(|input| input.raw.hovered_files.is_empty()) {
        return;
    }

    let hint = if app.session.has_source() {
        t!("drop.hint")
    } else {
        t!("drop.not_connected")
    };

    egui::Area::new(egui::Id::new("hovered_files"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(hint).strong());
            });
        });
}

/// Draws everything below this point at the size the status bar was asked for.
///
/// A bar is as tall as what stands in it, so the height of the setting is a
/// factor and not a number of points to pad with: the letters, the buttons, the
/// room between them and the bar of progress are all multiplied by it, and the
/// panel comes out the height that was asked for with nothing standing small in
/// the middle of it.
fn grow(ui: &mut egui::Ui, scale: f32) {
    if (scale - 1.0).abs() < f32::EPSILON {
        return;
    }

    let style = ui.style_mut();
    for font in style.text_styles.values_mut() {
        font.size *= scale;
    }
    style.spacing.interact_size *= scale;
    style.spacing.button_padding *= scale;
    style.spacing.item_spacing *= scale;
    style.spacing.icon_width *= scale;
    style.spacing.icon_width_inner *= scale;
}

/// A margin of the bar at that factor, in the whole points a margin is written
/// in and never so large that it stops being one.
fn margin(points: f32, scale: f32) -> i8 {
    (points * scale).round().clamp(0.0, 127.0) as i8
}

/// Draws one frame.
///
/// The status bar carries the way out of the settings, so it stands there even
/// when it is hidden everywhere else. The search bar takes its place while it
/// is open, so it is there even when the status bar is switched off.
pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let context = ui.ctx().clone();

    menu::draw(app, &context);
    ask::draw(app, &context);
    form::draw(app, &context);
    tasks::draw(app, &context);
    debug::draw(app, &context);
    confirm_file(app, &context);
    lost_source(app);
    hovered_files(app, &context);

    if app.search.open || app.settings.show_status_bar || app.ui.view == MainView::Settings {
        let scale = crate::config::status_bar_scale(app.settings.status_bar_height);
        egui::Panel::bottom("status_bar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(
                        margin(6.0, scale),
                        margin(3.0, scale),
                    )),
            )
            .show(ui, |ui| {
                grow(ui, scale);
                if app.search.open {
                    search::draw(app, ui, &context);
                } else {
                    statusbar::draw(app, ui, &context);
                }
            });
    }

    let area = ui.available_rect_before_wrap();
    let frame = egui::Frame::NONE.fill(ui.visuals().panel_fill);
    match app.ui.view {
        MainView::Terminal => {
            egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                // The window asking for the values of a connection is the
                // one thing being answered while it stands, so what is
                // behind it is drawn and answers nothing.
                if app.ui.ask.is_some() || app.ui.form.is_some() {
                    ui.disable();
                }
                terminal::draw(app, ui, &context);
            });
        }
        MainView::Settings => {
            egui::CentralPanel::default()
                .frame(frame.inner_margin(egui::Margin::same(8)))
                .show(ui, |ui| settings::draw(app, ui, &context));
        }
        MainView::FileDialog => {
            egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |_ui| {});
            file_dialog::draw(app, &context, area);
        }
    }

    block::draw(app, &context, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use zyt_config::ConfigStore;

    /// The whole interface, drawn without a window, with what the toolkit
    /// painted over it handed back.
    ///
    /// `egui` answers an identifier used twice in one frame by painting the
    /// complaint where the second one stood, so a test that reads the shapes
    /// reads the complaint: nothing of a window, a chip or a device is needed
    /// to find out that two widgets are standing in one place.
    fn painted(pointer: Option<egui::Pos2>) -> Vec<String> {
        painted_while(pointer, |_| {})
    }

    /// The same, with the application put into some state first.
    fn painted_while(
        pointer: Option<egui::Pos2>,
        prepare: impl FnOnce(&mut crate::app::App),
    ) -> Vec<String> {
        let (context, mut app) = application();
        prepare(&mut app);

        let mut found = Vec::new();
        for _ in 0..3 {
            found = frame(&context, &mut app, pointer);
        }
        found
    }

    /// An application drawing into no window, with a store of its own.
    fn application() -> (egui::Context, crate::app::App) {
        // A directory of its own per call: these tests run beside each other,
        // and a store two of them wrote to would be a store neither of them
        // described.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("zyterm-ui-{}-{count}", std::process::id()));
        let store = ConfigStore::with_paths(
            directory.join("config"),
            directory.join("data"),
            directory.join("lock"),
        );
        let context = egui::Context::default();
        let app = crate::app::App::new(&context, store, Settings::default(), None)
            .expect("the application starts");
        (context, app)
    }

    /// Draws one frame and says what the toolkit complained about in it.
    fn frame(
        context: &egui::Context,
        app: &mut crate::app::App,
        pointer: Option<egui::Pos2>,
    ) -> Vec<String> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            events: pointer
                .map(|at| vec![egui::Event::PointerMoved(at)])
                .unwrap_or_default(),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| draw(app, ui));
        output.textures_delta.clear();
        complaints(&output.shapes)
    }

    /// Every text the toolkit painted that reads as a complaint of its own.
    fn complaints(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => {
                    let text = text.galley.text();
                    if text.contains("Double use") || text.contains("🔥") {
                        out.push(format!("{text} at {:?}", shape_pos(shape)));
                    }
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, out)),
                _ => {}
            }
        }

        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    fn shape_pos(shape: &egui::Shape) -> egui::Pos2 {
        match shape {
            egui::Shape::Text(text) => text.pos,
            _ => egui::Pos2::ZERO,
        }
    }

    #[test]
    fn no_two_widgets_of_the_window_share_an_identifier() {
        assert_eq!(painted(None), Vec::<String>::new());
    }

    #[test]
    fn nor_do_they_while_a_selection_is_being_picked_out() {
        let selecting = |app: &mut crate::app::App| {
            app.session.terminal.feed(b"one\r\ntwo\r\nthree");
            app.session
                .terminal
                .select_by_key(zyt_term::SelectionStep::Left);
            app.session
                .terminal
                .select_by_key(zyt_term::SelectionStep::Up);
        };
        assert_eq!(painted_while(None, selecting), Vec::<String>::new());
        assert_eq!(
            painted_while(Some(egui::pos2(450.0, 300.0)), selecting),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nor_do_they_while_a_script_is_asking_something() {
        // The window of a dialog is drawn from what a script described, so the
        // one that is described here is the one with every kind of row in it:
        // the rows, the two ways out and the switch beside them all stand in
        // the same window.
        let asking = |app: &mut crate::app::App| {
            app.ui.form = Some(form::State::new(
                pending(described()),
                None,
                "shell-transfer".to_string(),
                "how".to_string(),
                None,
                true,
            ));
        };
        assert_eq!(painted_while(None, asking), Vec::<String>::new());
    }

    #[test]
    fn the_list_of_a_row_of_a_dialog_is_offered_when_that_row_is_pressed() {
        // The one way to that menu is the button of the row, because that is
        // the one the window stands in the way of: the dialog is taken out of
        // the state to be drawn, so a menu asked for while a row is being drawn
        // would be built from a dialog that is nowhere.
        let (context, mut app) = application();
        app.ui.form = Some(form::State::new(
            pending(described()),
            None,
            "shell-transfer".to_string(),
            "how".to_string(),
            None,
            true,
        ));
        frame(&context, &mut app, None);

        // The row that is pressed is the one picked from a menu, and nothing
        // else of the window says what an unanswered one of those says.
        let at = aim(&context, &mut app, t!("dialog.pick").as_ref());
        press(&context, &mut app, at);

        assert!(
            app.menu.is_open(),
            "the list of that row is offered once it is pressed"
        );
        frame(&context, &mut app, None);

        let order: Vec<egui::LayerId> =
            context.memory(|memory| memory.layer_ids().collect::<Vec<egui::LayerId>>());
        let window = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("script-form"));
        let at = |layer: egui::LayerId| {
            order
                .iter()
                .position(|standing| *standing == layer)
                .unwrap_or_else(|| panic!("{layer:?} is drawn: {order:?}"))
        };
        assert!(
            at(app.menu.layer_id()) > at(window),
            "the menu and the dialog are both drawn in the foreground, and the one \
             being answered is the menu: {order:?}"
        );
    }

    /// Where one text of the window stands once it stops moving.
    ///
    /// A window of this program is drawn in the middle of what it is given and
    /// is as wide as what is in it, and what is in it changes under a pointer —
    /// a list grows a bar to scroll by — so the first place a text was painted
    /// in is not where it stands once the pointer is over it. The pointer is put
    /// there and the place asked for again until it stops moving.
    fn aim(context: &egui::Context, app: &mut crate::app::App, wanted: &str) -> egui::Pos2 {
        let said = || format!("{wanted} is drawn");
        let mut at = painted_text(context, app, wanted).unwrap_or_else(|| panic!("{}", said()));
        for _ in 0..8 {
            frame(context, app, Some(at));
            let now = painted_text(context, app, wanted).unwrap_or_else(|| panic!("{}", said()));
            if (now - at).length() < 0.5 {
                break;
            }
            at = now;
        }
        at
    }

    /// Where one text the toolkit painted stands, when it painted it.
    fn painted_text(
        context: &egui::Context,
        app: &mut crate::app::App,
        wanted: &str,
    ) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, wanted: &str, out: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if out.is_none() && text.galley.text() == wanted => {
                    *out = Some(text.pos + text.galley.size() / 2.0);
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|shape| walk(shape, wanted, out));
                }
                _ => {}
            }
        }

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| draw(app, ui));
        output.textures_delta.clear();

        let mut found = None;
        for clipped in &output.shapes {
            walk(&clipped.shape, wanted, &mut found);
        }
        found
    }

    /// Presses the pointer where something stands and lets it go again.
    fn press(context: &egui::Context, app: &mut crate::app::App, at: egui::Pos2) {
        frame(context, app, Some(at));
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| draw(app, ui));
            output.textures_delta.clear();
        }
    }

    /// A dialog with one row of every kind in it.
    fn described() -> zyt_script::Form {
        use zyt_script::{Choice, Field, FieldKind};

        let options = vec![Choice::named("b64", "base64"), Choice::named("raw", "raw")];
        zyt_script::Form::new("Ask")
            .named("how")
            .and(Field::new(
                "remote",
                FieldKind::Text {
                    value: String::new(),
                    password: false,
                },
            ))
            .and(Field::new(
                "note",
                FieldKind::Textarea {
                    value: String::new(),
                    rows: 1,
                },
            ))
            .and(Field::new("verify", FieldKind::Switch { value: true }))
            .and(Field::new(
                "mode",
                FieldKind::OneOf {
                    options: options.clone(),
                    value: String::new(),
                },
            ))
            .and(Field::new(
                "digest",
                FieldKind::Select {
                    options: options.clone(),
                    value: None,
                },
            ))
            .and(Field::new(
                "steps",
                FieldKind::ManyOf {
                    options,
                    value: Vec::new(),
                },
            ))
            .and(Field::new("line", FieldKind::Separator))
            .and(Field::new("said", FieldKind::Note))
    }

    /// A dialog a script is waiting on, with the script on a thread of its own.
    ///
    /// Asking is one blocking call, so there is no way to one of these but to
    /// make the call; the thread is let go of when the answer goes nowhere.
    fn pending(form: zyt_script::Form) -> crate::scripts::Pending {
        use zyt_script::Prompt;

        let (talker, heard) = crate::scripts::talker(std::sync::Arc::new(|| {}));
        std::thread::spawn(move || {
            let _ = talker.ask(form);
        });
        heard.asks.recv().expect("the script asked")
    }

    #[test]
    fn nor_do_they_while_a_command_of_several_lines_is_being_written() {
        assert_eq!(
            painted_while(None, |app| {
                app.toggle_block_input();
                app.ui.block = Some("for i in 1 2\ndo\necho $i\ndone".to_string());
            }),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nor_do_they_while_the_pointer_rests_on_the_status_bar() {
        // The plates of the status bar rise under a pointer, and a plate that
        // is drawn is a plate whose identifiers are in that frame.
        for at in [
            egui::pos2(20.0, 580.0),
            egui::pos2(60.0, 580.0),
            egui::pos2(120.0, 580.0),
            egui::pos2(450.0, 300.0),
        ] {
            assert_eq!(painted(Some(at)), Vec::<String>::new(), "pointer at {at:?}");
        }
    }
}
