//! Small widgets the settings are built from.

/// A switch: a pill that slides when it is clicked.
///
/// It says on or off by its shape alone, which is what a column of settings
/// needs — a checkbox would carry the word "shown" beside every one of them.
///
/// This is the toggle switch of the widget gallery of the toolkit, kept as it
/// is written there — `egui_demo_lib/src/demo/toggle_switch.rs` — so it looks
/// and behaves like a control of the toolkit and follows the style: the size of
/// a button, the colors of a selectable widget, and the animation the toolkit
/// keeps for the state of an identifier. It also says what it is to a screen
/// reader, which is a checkbox.
pub fn switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, "")
    });

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(
            rect,
            radius,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    response
}

/// The one value of the settings that is open for writing, and what it held
/// when it was opened.
///
/// One at a time, because one keyboard. What it held is kept because what is
/// written is written into the value itself — so the field shows what is being
/// typed — and the caller is told to write it down only once, when the writing
/// is over and the value is not what it was.
#[derive(Debug, Default)]
pub struct Editing {
    open: Option<egui::Id>,
    was: String,
}

impl Editing {
    /// Whether this value is the one open for writing.
    fn writing(&self, id: egui::Id) -> bool {
        self.open == Some(id)
    }

    /// Opens one value for writing, remembering what it holds.
    fn open(&mut self, ui: &egui::Ui, id: egui::Id, value: &str) {
        self.open = Some(id);
        self.was = value.to_string();
        ui.memory_mut(|memory| memory.request_focus(id));
    }

    /// Closes it again, and says whether what it holds is another value.
    fn close(&mut self, value: &str) -> bool {
        self.open = None;
        self.was != value
    }
}

/// The pencil that opens one value for writing, and closes it again.
///
/// A page of fields is a page that looks like a form whether or not anything on
/// it is being changed, and a value nobody is changing is read and not written:
/// so the value is text, this opens it, and pressing the text itself does the
/// same — what somebody reaching for a value reaches for is the value.
///
/// It stands to the left of the value, in the one place a control can stand
/// without moving when the value beside it grows. It carries no hint of its
/// own: what it opens is the thing right beside it.
///
/// Writing ends when the field loses the keyboard — `Enter`, `Tab`, a press
/// anywhere else — because there is nothing else to end it with: a page of
/// values has no place for a button per value saying the writing is over. A
/// press beside it ends it whether or not the field ever took the keyboard, so
/// a field that was not given it does not stay open.
///
/// `shown` is what stands there while nothing is being written, which is not
/// always the value itself: a line that carries `{<name>}` is read with the
/// names put in and written with them still standing.
///
/// Answers true once, on the end of the writing, and only when what the value
/// holds is not what it held when it was opened. A letter typed is not a value
/// somebody settled on, and a file written per letter is a file written for
/// every way through a word that was never meant.
pub fn pencil(ui: &mut egui::Ui, editing: &mut Editing, id: egui::Id, value: &str) -> bool {
    let writing = editing.writing(id);
    if !ui
        .add(egui::Button::new(crate::ui::icons::EDIT).selected(writing))
        .clicked()
    {
        return false;
    }

    if writing {
        return editing.close(value);
    }
    editing.open(ui, id, value);
    false
}

/// The value the pencil beside it opens: text until it is opened, a field while
/// it is being written.
///
/// It is drawn apart from the pencil so that what stands between the two is the
/// caller's to decide — the directory of a console has a button there that opens
/// the picker, and it belongs beside the pencil and not past the value, which is
/// as wide as the value happens to be.
pub fn editable(
    ui: &mut egui::Ui,
    editing: &mut Editing,
    id: egui::Id,
    value: &mut String,
    shown: &str,
    hint: &str,
    width: f32,
) -> bool {
    if editing.writing(id) {
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .id(id)
                .hint_text(hint)
                .desired_width(width)
                .font(egui::TextStyle::Monospace),
        );
        if response.lost_focus() || response.clicked_elsewhere() {
            return editing.close(value);
        }
        return false;
    }

    let text = if shown.trim().is_empty() {
        egui::RichText::new(hint).weak()
    } else {
        egui::RichText::new(shown).monospace()
    };
    if ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::Text)
        .on_hover_text(hint)
        .clicked()
    {
        editing.open(ui, id, value);
    }
    false
}

/// Width a field needs to show `characters` of the monospaced family.
///
/// A field is asked for in characters because that is what goes in it — a name,
/// an address, a path — and a width in points says nothing about how much of
/// one is read without scrolling. The monospaced family is the one they are
/// written in, and every character of it is the same width, so one of them
/// answers for all.
pub fn monospace_width(ui: &egui::Ui, characters: usize) -> f32 {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let one = ui.ctx().fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
    one * characters as f32 + ui.spacing().item_spacing.x
}

/// A field of the width it is given, whatever the layout it stands in would
/// have offered it.
///
/// A `TextEdit` is never wider than the room around it — `desired_width` is
/// what it asks for and the layout has the last word — and a cell of a grid
/// offers what that column measured the frame before, which is what the field
/// in it was last given. The two hold each other there: a field asked to grow
/// is handed back the width it already had and grows by nothing, for ever. So
/// the room is allocated first, at the width the field is to have, and the
/// field is told to fill it.
pub fn sized_field(ui: &mut egui::Ui, width: f32, field: egui::TextEdit<'_>) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.add(field.desired_width(f32::INFINITY)),
    )
    .inner
}

/// Width a button of this text takes, so a row or a column can be measured for
/// one before it is drawn.
///
/// It is what the button comes out to and not a guess around it: the text laid
/// out in the family buttons are written in, and the padding the style puts on
/// either side of it. Nothing of the room between widgets is in it, because
/// that room belongs to whatever stands next to the button and not to the
/// button.
pub fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_string(), font, egui::Color32::WHITE)
            .size()
            .x
    });

    text + ui.spacing().button_padding.x * 2.0
}

/// A scroll area a finger scrolls by its contents and a pointer by its bar.
///
/// While a finger is on the screen the bar is not what scrolls the page. A bar
/// is a narrow strip at the edge of it, a finger is wide, and a press on the
/// track of a bar is not a step — it puts the handle where the press was — so a
/// swipe that happened to start on the bar does not scroll the page, it throws
/// it wherever the finger went, which on a page swiped from low down is the
/// bottom of it.
///
/// The contents are dragged instead, which is what a finger scrolls with
/// everywhere. The bar is itself again the moment the finger is lifted, so a
/// pointer drags it the way it always did, and it is drawn the whole time
/// either way: it is what says where the page stands.
pub fn scroll_area(ui: &egui::Ui) -> egui::ScrollArea {
    scrolled(ui, egui::ScrollArea::vertical())
}

/// The same area, scrolling both ways.
///
/// It is for what is as wide as it is long — a grid of rows whose widest one
/// decides the width — where cutting the width would hide a value and wrapping
/// it would move every row beside it.
pub fn scroll_area_both(ui: &egui::Ui) -> egui::ScrollArea {
    scrolled(ui, egui::ScrollArea::both())
}

/// What both of them share: who scrolls it, and with what.
fn scrolled(ui: &egui::Ui, area: egui::ScrollArea) -> egui::ScrollArea {
    let touching = ui.input(|input| input.any_touches());

    area.scroll_source(egui::containers::scroll_area::ScrollSource {
        scroll_bar: !touching,
        drag: egui::containers::scroll_area::DragScroll::OnTouch,
        mouse_wheel: true,
    })
}

/// Width a field of the monospaced family gives what is written in it, between
/// a floor and a ceiling.
///
/// A field as wide as the longest thing that could ever go in it is a field
/// that stands mostly empty, and one that never grows is a field that has to be
/// scrolled to be read. So it is as wide as what it holds: no narrower than
/// `least` characters, which is what an empty one shows its hint in, and no
/// wider than `most`, which is what the page has left to give it.
///
/// One character more than the text is kept on the end, so the caret has
/// somewhere to stand after the last letter that was typed.
pub fn written_width(ui: &egui::Ui, text: &str, least: usize, most: f32) -> f32 {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let (one, written) = ui.ctx().fonts_mut(|fonts| {
        let one = fonts.glyph_width(&font, '0');
        let written = fonts
            .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
            .size()
            .x;
        (one, written)
    });

    let floor = one * least as f32 + ui.spacing().item_spacing.x;
    let wanted = written + one + ui.spacing().item_spacing.x;
    wanted.clamp(floor, most.max(floor))
}

/// Width the widest of these labels needs, so a column of them lines up.
pub fn label_width(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let widest = labels
        .iter()
        .map(|text| {
            ui.ctx().fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap((*text).to_string(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            })
        })
        .fold(0.0_f32, f32::max);

    widest + ui.spacing().item_spacing.x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> egui::Context {
        egui::Context::default()
    }

    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        }
    }

    /// A field is as wide as it was told, in a grid as anywhere else.
    ///
    /// The cell of a grid offers the width that column measured the frame
    /// before, and a `TextEdit` never takes more than it is offered, so a field
    /// that asked through `desired_width` alone is handed back the width it
    /// already had and grows by nothing.
    #[test]
    fn a_field_in_a_grid_is_as_wide_as_it_is_told() {
        let context = context();
        let (mut narrow, mut wide) = (String::new(), String::new());
        let mut widths = Vec::new();

        let mut output = context.run_ui(input(), |ui| {
            egui::Grid::new("fields").num_columns(3).show(ui, |ui| {
                widths.push(
                    sized_field(ui, 120.0, egui::TextEdit::singleline(&mut narrow))
                        .rect
                        .width(),
                );
                widths.push(
                    sized_field(ui, 420.0, egui::TextEdit::singleline(&mut wide))
                        .rect
                        .width(),
                );
                ui.label("");
                ui.end_row();
            });
        });
        output.textures_delta.clear();

        // A window may lay its contents out more than once before it draws
        // them, and each pass measures the two fields again.
        assert!(!widths.is_empty());
        assert!(
            widths.chunks(2).all(|pass| pass == [120.0, 420.0]),
            "{widths:?}"
        );
    }

    /// The width follows what is written, between the floor of an empty field
    /// and the ceiling the page allows.
    #[test]
    fn a_field_is_as_wide_as_what_is_written_in_it() {
        let context = context();

        let mut output = context.run_ui(input(), |ui| {
            let most = monospace_width(ui, 40);
            let empty = written_width(ui, "", 12, most);
            let short = written_width(ui, "abc", 12, most);
            let long = written_width(ui, &"a".repeat(20), 12, most);
            let longer = written_width(ui, &"a".repeat(24), 12, most);
            let past = written_width(ui, &"a".repeat(400), 12, most);

            assert_eq!(empty, short, "what is shorter than the floor gets it");
            assert!(long > empty, "and what is longer grows past it");
            assert!(longer > long, "one character at a time");
            assert_eq!(past, most, "up to what the page has to give");
        });
        output.textures_delta.clear();
    }
}
