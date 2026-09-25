//! The terminal widget.

use crate::cache::{Painted, TerminalCache};
use crate::font::TerminalFont;
use crate::scrollbar;
use crate::theme::TerminalTheme;
use egui::epaint::{Mesh, Tessellator};
use egui::{Align2, Color32, Rect, Sense, Shape, Stroke, Ui, Vec2};
use zyt_term::{
    Cell, CursorShape, MouseButton, RenderableContent, SelectionKind, Terminal, encode_mouse,
};

/// Whether a cell carries a character worth drawing.
fn drawn(cell: &Cell) -> bool {
    !cell.style.wide_spacer && cell.ch != ' ' && cell.ch != '\0'
}

/// Width of the scrollbar on the right edge.
const SCROLLBAR_WIDTH: f32 = 12.0;
/// Width of the bar while the pointer is elsewhere.
const SCROLLBAR_IDLE_WIDTH: f32 = 4.0;

/// A hyperlink the pointer is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// Address of the link.
    pub uri: String,
    /// Text shown for it.
    pub text: String,
}

/// What the widget produced during one frame.
#[derive(Debug, Default, Clone)]
pub struct TerminalOutput {
    /// Bytes that must be written to the device.
    pub bytes: Vec<u8>,
    /// New grid size when the widget area changed.
    pub resized: Option<(usize, usize)>,
    /// Text of a selection the user asked to copy.
    pub copied: Option<String>,
    /// Link under the pointer.
    pub hovered_link: Option<LinkTarget>,
    /// Link the user opened with the left button.
    pub opened_link: Option<LinkTarget>,
    /// Link the user asked a menu for with the right button.
    pub link_menu: Option<LinkTarget>,
    /// True while the program reads the mouse itself.
    pub wants_mouse: bool,
    /// The middle button was clicked, which pastes in a terminal.
    pub paste_requested: bool,
}

/// Immediate mode terminal widget.
pub struct TerminalView<'a> {
    terminal: &'a mut Terminal,
    content: &'a mut RenderableContent,
    cache: &'a mut TerminalCache,
    theme: &'a TerminalTheme,
    font: &'a TerminalFont,
    focused: bool,
    auto_resize: bool,
    links: bool,
    program_colors: bool,
    mouse_reports: bool,
    selection_anchor: bool,
}

impl<'a> TerminalView<'a> {
    /// Widget for the given terminal, drawing into the reused snapshot.
    ///
    /// The cache keeps the shapes of the grid between frames, so a frame that
    /// would paint the same picture again paints the kept one.
    pub fn new(
        terminal: &'a mut Terminal,
        content: &'a mut RenderableContent,
        cache: &'a mut TerminalCache,
        theme: &'a TerminalTheme,
        font: &'a TerminalFont,
    ) -> Self {
        Self {
            terminal,
            content,
            cache,
            theme,
            font,
            focused: true,
            auto_resize: true,
            links: true,
            program_colors: true,
            mouse_reports: true,
            selection_anchor: true,
        }
    }

    /// Marks the widget as focused, which changes the cursor shape.
    ///
    /// The cursor is drawn over the kept picture and not into it, so this
    /// changes nothing the picture is built from and costs no frame.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Enables or disables resizing the grid to the widget area.
    pub fn auto_resize(mut self, auto_resize: bool) -> Self {
        self.auto_resize = auto_resize;
        self
    }

    /// Enables or disables the colors a program set for itself.
    ///
    /// Disabled, the palette of the caller is what the terminal wears, whatever
    /// the program asked for.
    pub fn program_colors(mut self, program_colors: bool) -> Self {
        self.program_colors = program_colors;
        self
    }

    /// Answers the mouse reports a program asked for, or keeps the mouse here.
    ///
    /// Disabled, a program that asked for the mouse is not answered and the
    /// pointer does what it does in a terminal nobody asked anything of: it
    /// selects, it scrolls and it leaves the buttons to the caller. What the
    /// program asked for is not changed by it — it is left unanswered — so
    /// enabling it again hands the mouse straight back.
    pub fn mouse_reports(mut self, mouse_reports: bool) -> Self {
        self.mouse_reports = mouse_reports;
        self
    }

    /// The modes the widget acts on, which are the modes of the terminal with
    /// the mouse taken out while the caller keeps it.
    fn modes(&self) -> zyt_term::TerminalModes {
        let mut modes = self.terminal.modes();
        modes.mouse_report &= self.mouse_reports;
        modes
    }

    /// Shows or hides the bar that says where a selection would begin.
    ///
    /// Hidden, the place is still kept and `Shift` and a press still select
    /// from it: what goes away is the mark of it and nothing else. It is a
    /// thing drawn over the grid wherever the last press landed, which is a
    /// mark some readers want and others read as a cursor of a second kind, so
    /// the caller says which it is.
    pub fn selection_anchor(mut self, selection_anchor: bool) -> Self {
        self.selection_anchor = selection_anchor;
        self
    }

    /// Enables or disables the links a program reported.
    ///
    /// Disabled, a link is neither underlined nor reported to the caller, so
    /// the text stays text.
    pub fn links(mut self, links: bool) -> Self {
        self.links = links;
        self
    }

    /// Underlines the links of one row, dotted.
    ///
    /// A link is underlined where it stands, so the underline belongs to the
    /// row it runs through and is kept with it. The link under the pointer is
    /// underlined solid over the top of this, which is what covers the dots.
    fn paint_row_links(
        &self,
        shapes: &mut Vec<Shape>,
        cells: &[Cell],
        origin: egui::Pos2,
        cell: Vec2,
        row: usize,
    ) {
        if !self.links || self.content.links.is_empty() {
            return;
        }

        for (column, grid_cell) in cells.iter().enumerate() {
            if grid_cell.link.is_none() {
                continue;
            }

            let y = origin.y + (row as f32 + 1.0) * cell.y - 1.0;
            let left = origin.x + column as f32 * cell.x;
            let color = self.resolve(grid_cell.fg);

            let mut dot = left;
            while dot < left + cell.x {
                let end = (dot + 1.5).min(left + cell.x);
                shapes.push(Shape::line_segment(
                    [egui::pos2(dot, y), egui::pos2(end, y)],
                    Stroke::new(1.0, color),
                ));
                dot += 3.0;
            }
        }
    }

    /// Underlines the link under the pointer, solid, wherever it stands.
    ///
    /// It is drawn beside the kept picture and not into it: the pointer finds
    /// another link as often as it is moved, and a picture that had to be built
    /// again for it would be built again for every link the pointer crosses.
    /// A handful of lines a frame is nothing, and the dotted underline of the
    /// link, which belongs to the text, is covered by them.
    fn paint_hovered_link(
        &self,
        painter: &egui::Painter,
        origin: egui::Pos2,
        cell: Vec2,
        hovered: Option<&LinkTarget>,
    ) {
        if !self.links {
            return;
        }
        let Some(target) = hovered else {
            return;
        };

        let columns = self.content.columns.max(1);
        for (index, grid_cell) in self.content.cells.iter().enumerate() {
            let Some(id) = grid_cell.link else {
                continue;
            };
            if self.content.links.get(usize::from(id.0)) != Some(&target.uri) {
                continue;
            }

            let row = index / columns;
            let column = index % columns;
            let y = origin.y + (row as f32 + 1.0) * cell.y - 1.0;
            let left = origin.x + column as f32 * cell.x;

            painter.line_segment(
                [egui::pos2(left, y), egui::pos2(left + cell.x, y)],
                Stroke::new(1.0, self.resolve(grid_cell.fg)),
            );
        }
    }

    /// Tells a program that reads the mouse where the pointer went.
    ///
    /// A press and a release alone are not enough for an editor: a selection
    /// with the mouse needs the motion between them, which is reported while a
    /// button is held (1002) or at all times (1003). Only a change of cell is
    /// reported, so a slow line is not flooded.
    #[allow(clippy::too_many_arguments)]
    fn report_motion(
        &self,
        ui: &Ui,
        held: Option<egui::PointerButton>,
        column: usize,
        row: usize,
        modifiers: zyt_term::Modifiers,
        modes: zyt_term::TerminalModes,
        output: &mut TerminalOutput,
    ) {
        if !modes.mouse_drag && !modes.mouse_motion {
            return;
        }
        if held.is_none() && !modes.mouse_motion {
            return;
        }

        let id = ui.id().with("terminal_pointer_cell");
        let last: Option<(usize, usize)> = ui.data(|data| data.get_temp(id));
        if last == Some((column, row)) {
            return;
        }
        ui.data_mut(|data| data.insert_temp(id, (column, row)));

        let button = match held {
            Some(egui::PointerButton::Primary) => MouseButton::LeftMove,
            Some(egui::PointerButton::Middle) => MouseButton::MiddleMove,
            Some(egui::PointerButton::Secondary) => MouseButton::RightMove,
            _ => MouseButton::NoneMove,
        };
        if let Some(bytes) = encode_mouse(button, true, column, row, modifiers, modes) {
            output.bytes.extend(bytes);
        }
    }

    /// Background in use: the one the program set, else the one of the theme.
    fn background(&self) -> egui::Color32 {
        match self.content.background.filter(|_| self.program_colors) {
            Some(color) => egui::Color32::from_rgb(color.r, color.g, color.b),
            None => self.theme.background,
        }
    }

    /// Foreground in use: the one the program set, else the one of the theme.
    fn foreground(&self) -> egui::Color32 {
        match self.content.foreground.filter(|_| self.program_colors) {
            Some(color) => egui::Color32::from_rgb(color.r, color.g, color.b),
            None => self.theme.foreground,
        }
    }

    /// Color of the cursor: the one the program set for it, the foreground it
    /// set where it set no cursor color, and the cursor color of the theme
    /// where it set neither or may not have either.
    ///
    /// A program that paints the screen in its own pair and leaves the cursor
    /// alone would otherwise draw it in a color of the theme against a
    /// background of its own, which is how a cursor goes missing.
    fn cursor_color(&self) -> egui::Color32 {
        if self.program_colors {
            if let Some(color) = self.content.cursor_color {
                return egui::Color32::from_rgb(color.r, color.g, color.b);
            }
            if self.content.foreground.is_some() {
                return self.foreground();
            }
        }
        self.theme.cursor
    }

    /// One entry of the color table as the program painted it, where it did and
    /// where it may.
    ///
    /// The table is empty until a program paints over it, so an ordinary
    /// snapshot answers nothing here and every cell is drawn in the colors of
    /// the theme.
    fn painted(&self, index: u8) -> Option<egui::Color32> {
        if !self.program_colors {
            return None;
        }
        let color = (*self.content.palette.get(usize::from(index))?)?;
        Some(egui::Color32::from_rgb(color.r, color.g, color.b))
    }

    /// One cell color, with what the program set taken into account.
    fn resolve(&self, color: zyt_term::Color) -> egui::Color32 {
        match color {
            zyt_term::Color::Background => self.background(),
            zyt_term::Color::Foreground => self.foreground(),
            zyt_term::Color::Palette(index) | zyt_term::Color::Indexed(index) => self
                .painted(index)
                .unwrap_or_else(|| self.theme.resolve(color)),
            other => self.theme.resolve(other),
        }
    }

    /// Draws the widget and handles mouse input.
    pub fn show(mut self, ui: &mut Ui) -> (egui::Response, TerminalOutput) {
        let mut output = TerminalOutput::default();
        let cell = self.font.cell_size(ui.ctx());
        let area = ui.available_size();
        let (response, painter) = ui.allocate_painter(area, self.sense());
        let full_rect = response.rect;
        let origin = full_rect.min;

        if self.auto_resize {
            let (columns, rows) = self.font.grid_size(ui.ctx(), full_rect.size());
            if self.terminal.size() != (columns, rows)
                && self.terminal.resize(columns, rows).is_ok()
            {
                output.resized = Some((columns, rows));
            }
        }

        self.keep_keyboard(ui, &response);

        let grabs_mouse = self.modes().mouse_report;
        let on_scrollbar = !grabs_mouse
            && self
                .scrollbar_geometry(full_rect)
                .map(|(track, _)| ui.rect_contains_pointer(track))
                .unwrap_or(false);
        if !on_scrollbar {
            self.handle_mouse(ui, &response, cell, origin, &mut output);
        }
        if !grabs_mouse {
            self.handle_scrollbar(ui, full_rect);
        }
        if self.terminal.is_dirty()
            || self.cache.is_empty()
            || self.content.columns != self.terminal.size().0
        {
            self.cache.read_page(self.terminal, self.content);
        }

        self.paint_cached_grid(ui, &painter, full_rect, cell);
        self.paint_cursor(&painter, full_rect.min, cell);
        self.paint_selection_anchor(&painter, full_rect.min, cell);
        self.paint_hovered_link(&painter, origin, cell, output.hovered_link.as_ref());
        if output.hovered_link.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        self.paint_scrollbar(ui, &painter, full_rect);

        output.wants_mouse = self.modes().mouse_report;
        output.bytes.extend(self.terminal.take_output());
        (response, output)
    }

    /// What the widget senses.
    ///
    /// The mouse is answered either way, but only the focused part of the
    /// interface stands in the focus order of the toolkit: a terminal that is
    /// not it would take the keyboard from whatever is — Tab and the arrow keys
    /// walk every focusable widget of the frame — and never give it back,
    /// because [`Self::keep_keyboard`] leaves it where it found it.
    fn sense(&self) -> Sense {
        if self.focused {
            Sense::click_and_drag()
        } else {
            Sense::CLICK | Sense::DRAG
        }
    }

    /// Keeps the keyboard inside the terminal.
    ///
    /// The toolkit moves the focus with Tab and the arrow keys, which would
    /// take those keys away from the device. While the terminal is the focused
    /// part of the interface it therefore holds the toolkit focus as well and
    /// locks those keys to itself; the caller decides when to hand the keyboard
    /// somewhere else.
    fn keep_keyboard(&self, ui: &Ui, response: &egui::Response) {
        if !self.focused {
            return;
        }

        let mine = ui.memory(|memory| memory.has_focus(response.id));
        if !mine && (response.clicked() || ui.memory(|memory| memory.focused().is_none())) {
            response.request_focus();
        }

        if ui.memory(|memory| memory.has_focus(response.id)) {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
        }
    }

    /// Geometry of the scrollbar track and its thumb, when there is history.
    fn scrollbar_geometry(&self, rect: Rect) -> Option<(Rect, Rect)> {
        let history = self.terminal.history_size();
        if history == 0 {
            return None;
        }

        let track = Rect::from_min_max(
            egui::pos2(rect.max.x - SCROLLBAR_WIDTH, rect.min.y),
            rect.max,
        );
        let metrics = scrollbar::metrics(
            track.height(),
            self.terminal.size().1,
            history,
            self.terminal.display_offset(),
        );
        let thumb = Rect::from_min_size(
            egui::pos2(track.min.x + 2.0, track.min.y + metrics.top),
            Vec2::new(SCROLLBAR_WIDTH - 4.0, metrics.height),
        );
        Some((track, thumb))
    }

    fn handle_scrollbar(&mut self, ui: &Ui, rect: Rect) {
        let Some((track, thumb)) = self.scrollbar_geometry(rect) else {
            return;
        };

        // The left button and no other: the bar is a control the pointer
        // takes hold of, and the right button held over it is the page being
        // dragged, which is another thing entirely.
        let response = ui.interact(track, ui.id().with("terminal_scrollbar"), self.sense());
        if !response.dragged_by(egui::PointerButton::Primary)
            && !response.clicked_by(egui::PointerButton::Primary)
        {
            return;
        }
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };

        let offset = scrollbar::offset_at(
            track.height(),
            thumb.height(),
            self.terminal.history_size(),
            pointer.y - track.min.y,
        );
        self.terminal.scroll_to(offset);
    }

    /// Draws the bar over the text, so it takes no column of its own and
    /// nothing of the interface stands beside a background a program painted.
    fn paint_scrollbar(&self, ui: &Ui, painter: &egui::Painter, rect: Rect) {
        let Some((track, thumb)) = self.scrollbar_geometry(rect) else {
            return;
        };
        let hovered = ui.rect_contains_pointer(track);

        let width = if hovered {
            SCROLLBAR_WIDTH - 4.0
        } else {
            SCROLLBAR_IDLE_WIDTH
        };
        let bar = Rect::from_min_size(
            egui::pos2(track.max.x - width - 2.0, thumb.min.y),
            Vec2::new(width, thumb.height()),
        );
        let color = if hovered {
            self.foreground().gamma_multiply(0.7)
        } else {
            self.foreground().gamma_multiply(0.4)
        };
        painter.rect_filled(bar, 3.0, color);
    }

    fn handle_mouse(
        &mut self,
        ui: &Ui,
        response: &egui::Response,
        cell: Vec2,
        origin: egui::Pos2,
        output: &mut TerminalOutput,
    ) {
        let modes = self.modes();
        let position = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos())
            .map(|pointer| grid_position(pointer, origin, cell, self.terminal.size()));

        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() >= 1.0 && response.hovered() {
            let lines = (scroll / cell.y).round() as i32;
            if lines != 0 {
                if modes.mouse_report && modes.alt_screen {
                    let button = if lines > 0 {
                        MouseButton::WheelUp
                    } else {
                        MouseButton::WheelDown
                    };
                    if let Some((column, row, _)) = position {
                        for _ in 0..lines.abs().min(5) {
                            if let Some(bytes) = encode_mouse(
                                button,
                                true,
                                column,
                                row,
                                zyt_term::Modifiers::default(),
                                modes,
                            ) {
                                output.bytes.extend(bytes);
                            }
                        }
                    }
                } else {
                    self.terminal.scroll(lines);
                }
            }
        }

        if self.touch_selection(ui, response, position, output) {
            return;
        }

        let Some((column, row, right_half)) = position else {
            return;
        };

        let held = ui.input(|input| input.modifiers);
        let alt = alt_held(ui, &held);
        if modes.mouse_report && !forced_block(&held, alt) {
            let modifiers = crate::input_map::map_modifiers(&held);
            let mut held = None;

            for button in [
                egui::PointerButton::Primary,
                egui::PointerButton::Middle,
                egui::PointerButton::Secondary,
            ] {
                let (pressed, released, down) = ui.input(|input| {
                    (
                        input.pointer.button_pressed(button),
                        input.pointer.button_released(button),
                        input.pointer.button_down(button),
                    )
                });
                if down {
                    held = Some(button);
                }
                if !pressed && !released {
                    continue;
                }
                // A selection is the window's and not the program's, so a press
                // puts it away here exactly as it does when nothing reads the
                // mouse: the text under it was chosen with ctrl+shift held, or
                // before the program asked for the mouse at all, and leaving it
                // marked would say it is still the selection.
                if pressed && button == egui::PointerButton::Primary {
                    self.terminal.selection_clear();
                }
                if let Some(mapped) = crate::input_map::map_button(button)
                    && let Some(bytes) =
                        encode_mouse(mapped, pressed, column, row, modifiers, modes)
                {
                    output.bytes.extend(bytes);
                }
            }

            self.report_motion(ui, held, column, row, modifiers, modes, output);
            return;
        }

        if response.clicked_by(egui::PointerButton::Middle) {
            output.paste_requested = true;
        }
        self.drag_scroll(ui, response, cell);

        // While the keys that extend a selection are held, a press on a link is
        // a press on the text under it: `Shift` says this press is about the
        // selection.
        let extending = extends_selection(&held, alt);

        if self.links
            && let Some(target) = link_at(self.content, column, row)
        {
            output.hovered_link = Some(target.clone());
            if response.clicked_by(egui::PointerButton::Primary) && !extending {
                output.opened_link = Some(target.clone());
            }
            if response.clicked_by(egui::PointerButton::Secondary) {
                output.link_menu = Some(target);
            }
        }

        let clicks = self.count_clicks(ui);
        let block = block_selection(&held, alt);
        if response.drag_started_by(egui::PointerButton::Primary) {
            if extending {
                let _ = self.terminal.selection_extend(column, row, right_half);
            } else {
                let kind = drag_kind(clicks, block);
                let _ = self.terminal.selection_start(kind, column, row);
            }
        } else if response.dragged_by(egui::PointerButton::Primary) {
            let _ = self.terminal.selection_update(column, row, right_half);
        } else if response.drag_stopped_by(egui::PointerButton::Primary) {
            output.copied = self.terminal.selected_text();
        } else if response.triple_clicked_by(egui::PointerButton::Primary) {
            let _ = self
                .terminal
                .selection_start(SelectionKind::Lines, column, row);
            output.copied = self.terminal.selected_text();
        } else if response.double_clicked_by(egui::PointerButton::Primary) {
            let _ = self
                .terminal
                .selection_start(SelectionKind::Semantic, column, row);
            output.copied = self.terminal.selected_text();
        } else if response.clicked_by(egui::PointerButton::Primary) && extending {
            let _ = self.terminal.selection_extend(column, row, right_half);
            output.copied = self.terminal.selected_text();
        } else if response.clicked_by(egui::PointerButton::Primary) && output.opened_link.is_none()
        {
            self.terminal.selection_clear();
            let _ = self.terminal.set_selection_anchor(column, row);
        }
    }

    /// Selects with a finger held still on the text, and answers whether the
    /// finger is doing that now.
    ///
    /// Holding is how a touch screen asks for what a pointer asks for by
    /// pressing and dragging. The toolkit calls it a long touch, and it does
    /// two things with it: it takes the drag away — a finger that has been held
    /// is not dragging any more — and it reports it where a right click is
    /// reported. Neither is what a terminal wants, so the selection is carried
    /// here: the word under the finger is taken when the hold is recognised,
    /// and what the finger moves over after that is added to it, read from
    /// where the pointer is rather than from a drag that is not there.
    ///
    /// It is a finger and nothing else. A long touch is a right click to the
    /// toolkit, and the right button held on this widget drags the page: a
    /// selection that took the toolkit at its word would begin under a pointer
    /// that is scrolling. `Response::long_touched` is the one answer that says
    /// a finger and not a button.
    ///
    /// The word and not the character, because a finger covers several of them
    /// and what somebody holds a finger on is a word.
    fn touch_selection(
        &mut self,
        ui: &Ui,
        response: &egui::Response,
        position: Option<(usize, usize, bool)>,
        output: &mut TerminalOutput,
    ) -> bool {
        let id = response.id.with("touch_selection");
        let selecting = ui.data_mut(|data| data.get_temp::<bool>(id).unwrap_or(false));

        if !selecting {
            let Some((column, row, _)) = position else {
                return false;
            };
            if self.modes().mouse_report || !response.long_touched() {
                return false;
            }

            let _ = self
                .terminal
                .selection_start(SelectionKind::Semantic, column, row);
            ui.data_mut(|data| data.insert_temp(id, true));
            return true;
        }

        let holding = ui.input(|input| input.any_touches() && input.pointer.primary_down());
        if let Some((column, row, right_half)) = position
            && holding
        {
            let _ = self.terminal.selection_update(column, row, right_half);
            return true;
        }

        // The finger is up, and with it goes the last thing the toolkit says
        // about where it was: the selection is finished where it stood.
        ui.data_mut(|data| data.remove_temp::<bool>(id));
        output.copied = self.terminal.selected_text();
        true
    }

    /// Scrolls the page by the right button held down and the pointer moved.
    ///
    /// The page follows the pointer rather than running from it: what is
    /// dragged is the text itself, the way a finger moves it, so a pointer
    /// pulled down brings back what stood above. It is the right button
    /// because the left one selects and the middle one pastes, and it is only
    /// the drag: a right button pressed and let go where it was opened a menu
    /// before this and opens one still.
    ///
    /// What the pointer has moved is kept between frames in points and spent in
    /// whole lines, because a terminal scrolls in lines: a pointer moved by
    /// less than the height of one has not scrolled nothing, it has scrolled
    /// part of a line, and throwing that part away on every frame is a drag
    /// that moves a page by nothing.
    fn drag_scroll(&mut self, ui: &Ui, response: &egui::Response, cell: Vec2) {
        let id = response.id.with("drag_scroll");
        if !response.dragged_by(egui::PointerButton::Secondary) {
            ui.data_mut(|data| data.remove_temp::<f32>(id));
            return;
        }

        let moved = ui.input(|input| input.pointer.delta().y);
        let lines = ui.data_mut(|data| {
            let carried: &mut f32 = data.get_temp_mut_or_default(id);
            let (lines, left) = dragged_lines(*carried, moved, cell.y);
            *carried = left;
            lines
        });

        self.terminal.scroll(lines);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    /// How many times the primary button was pressed in a row, counted here.
    ///
    /// The toolkit decides a double click on the release, which is after the
    /// drag of that same press has already started, so the count it offers
    /// comes too late to choose what a drag selects. This counts on the press
    /// instead, by the same delay and distance the toolkit uses, and answers
    /// the count of the press the pointer is still holding.
    fn count_clicks(&self, ui: &Ui) -> u8 {
        let id = ui.id().with("terminal_click_run");
        let (delay, distance) = ui.ctx().options(|options| {
            (
                options.input_options.max_double_click_delay,
                options.input_options.max_click_dist,
            )
        });
        let (time, position, pressed) = ui.input(|input| {
            (
                input.time,
                input.pointer.interact_pos(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
            )
        });

        let last: Option<ClickRun> = ui.data(|data| data.get_temp(id));
        if !pressed {
            return last.map(|run| run.count).unwrap_or(1);
        }

        let Some(position) = position else {
            return 1;
        };
        let count = match last {
            Some(run)
                if time - run.time < delay
                    && run.position.distance(position) < distance
                    && run.count < 3 =>
            {
                run.count + 1
            }
            _ => 1,
        };
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                ClickRun {
                    time,
                    position,
                    count,
                },
            )
        });
        count
    }

    fn paint_cached_grid(&mut self, ui: &Ui, painter: &egui::Painter, area: Rect, cell: Vec2) {
        let pixels_per_point = ui.ctx().pixels_per_point();
        let font_texture = ui.ctx().fonts(|fonts| fonts.font_image_size());
        let painted = Painted {
            revision: self.cache.revision(),
            area,
            cell,
            pixels_per_point,
            font_texture,
            font: self.font.clone(),
            theme: self.theme.clone(),
            program_colors: self.program_colors,
            links: self.links,
        };

        if !self.cache.holds(&painted) {
            let (mesh, atlas) = self.build_picture(ui, painter, area, cell);
            self.cache.keep(
                Painted {
                    font_texture: atlas,
                    ..painted
                },
                mesh,
            );
            if atlas != font_texture {
                ui.ctx().request_repaint();
            }
        }

        if let Some(mesh) = self.cache.mesh() {
            painter.add(Shape::Mesh(mesh));
        }
    }

    /// Cuts the whole grid into one mesh: the background, the cells with their
    /// lines and the underlines of their links, and the size of the atlas its
    /// glyphs were named against.
    ///
    /// The cursor and the link under the pointer are not in it — they are drawn
    /// over it, every frame, because each of them moves while the page stands
    /// still.
    ///
    /// The shapes are made first and cut afterwards. Laying a character out is
    /// what grows the atlas, and a mesh names its glyphs as a share of it, so
    /// the size is read once every character of the page has been laid out and
    /// the whole picture is cut against a size that stands.
    fn build_picture(
        &mut self,
        ui: &Ui,
        painter: &egui::Painter,
        area: Rect,
        cell: Vec2,
    ) -> (Mesh, [usize; 2]) {
        let mut shapes = vec![Shape::rect_filled(area, 0.0, self.background())];
        let font_id = self.font.font_id();
        for (row, cells) in self.content.rows().take(self.content.rows).enumerate() {
            self.paint_row(painter, &mut shapes, cells, area.min, cell, row, &font_id);
        }

        let atlas = painter.ctx().fonts(|fonts| fonts.font_image_size());
        let mut tessellator = Tessellator::new(
            ui.ctx().pixels_per_point(),
            ui.ctx().tessellation_options(|options| *options),
            atlas,
            Vec::new(),
        );
        let mut mesh = self.cache.take_mesh();
        for shape in shapes {
            tessellator.tessellate_shape(shape, &mut mesh);
        }
        (mesh, atlas)
    }

    /// Draws one row.
    ///
    /// Cells that agree on their colors and their style are one run, so a row
    /// of ordinary text is one rectangle of background and nothing else. Each
    /// character is placed in the middle of its own cell: the toolkit lays a
    /// string out by advancing a cursor glyph by glyph and rounding it to the
    /// pixel grid, so a row laid out as one piece drifts off the cells, and a
    /// selection cutting the row would start that drift again from another
    /// place and make the letters jump.
    ///
    /// A cell is painted in its own rectangle and nowhere else. A grid of whole
    /// cells rarely fills the widget exactly, and what is left over beside it
    /// keeps the background of the widget: a line that paints itself is a line
    /// and not a border, so its color stops where its last cell does.
    #[expect(
        clippy::too_many_arguments,
        reason = "where a row goes and what draws it"
    )]
    fn paint_row(
        &self,
        painter: &egui::Painter,
        shapes: &mut Vec<Shape>,
        cells: &[Cell],
        origin: egui::Pos2,
        cell: Vec2,
        row: usize,
        font_id: &egui::FontId,
    ) {
        let top = origin.y + row as f32 * cell.y;
        let mut column = 0;
        while column < cells.len() {
            let current = &cells[column];
            if current.style.wide_spacer {
                column += 1;
                continue;
            }

            let (fg, bg) = self.cell_colors(current);
            let mut run_end = column + 1;
            while let Some(next) = cells.get(run_end) {
                if !next.style.wide_spacer
                    && (self.cell_colors(next) != (fg, bg) || next.style != current.style)
                {
                    break;
                }
                run_end += 1;
            }

            let rect = Rect::from_min_size(
                egui::pos2(origin.x + column as f32 * cell.x, top),
                Vec2::new((run_end - column) as f32 * cell.x, cell.y),
            );
            if bg != self.background() {
                shapes.push(Shape::rect_filled(rect, 0.0, bg));
            }

            if !current.style.hidden {
                for (index, glyph) in cells[column..run_end].iter().enumerate() {
                    if !drawn(glyph) {
                        continue;
                    }
                    let at = egui::pos2(
                        origin.x + (column + index) as f32 * cell.x + cell.x / 2.0,
                        top,
                    );
                    let galley = painter.layout_no_wrap(glyph.ch.to_string(), font_id.clone(), fg);
                    if galley.is_empty() {
                        continue;
                    }
                    let placed = Align2::CENTER_TOP.anchor_size(at, galley.size());
                    shapes.push(Shape::galley(placed.min, galley, fg));
                }
                self.paint_lines(shapes, rect, fg, current);
            }

            column = run_end;
        }

        self.paint_row_links(shapes, cells, origin, cell, row);
    }

    fn paint_lines(&self, shapes: &mut Vec<Shape>, rect: Rect, color: Color32, cell: &Cell) {
        if cell.style.underline {
            let y = rect.bottom() - 1.0;
            shapes.push(Shape::line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(1.0, color),
            ));
        }
        if cell.style.strikeout {
            let y = rect.center().y;
            shapes.push(Shape::line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(1.0, color),
            ));
        }
    }

    fn cell_colors(&self, cell: &Cell) -> (Color32, Color32) {
        let mut fg = self.resolve(cell.fg);
        let mut bg = self.resolve(cell.bg);
        if cell.style.inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.style.dim {
            fg = fg.gamma_multiply(0.6);
        }
        if cell.matched {
            bg = self.theme.search_match;
        }
        if cell.selected {
            bg = self.theme.selection;
        }
        (fg, bg)
    }

    /// Draws the bar that says where a selection would begin.
    ///
    /// It stands before the character it names, at the edge between that one
    /// and the one before it, because what it marks is a place between two
    /// characters and not a character: a selection started there takes the one
    /// the bar stands in front of, and a bar drawn over a cell would say it
    /// takes that cell whichever way the pointer went.
    ///
    /// It is drawn over the picture and never into it, the way the cursor is:
    /// it moves with a press and the page under it does not change, so a
    /// picture built again for it would be a picture built for a click.
    fn paint_selection_anchor(&self, painter: &egui::Painter, origin: egui::Pos2, cell: Vec2) {
        if !self.selection_anchor {
            return;
        }
        let Some((column, row)) = self.content.selection_anchor else {
            return;
        };

        let width = (cell.x * 0.12).clamp(1.0, 3.0);
        let position = origin + Vec2::new(column as f32 * cell.x, row as f32 * cell.y);
        let bar = Rect::from_min_size(position, Vec2::new(width, cell.y));

        painter.add(Shape::rect_filled(bar, 0.0, self.theme.cursor));
    }

    fn paint_cursor(&self, painter: &egui::Painter, origin: egui::Pos2, cell: Vec2) {
        let Some(cursor) = self.content.cursor else {
            return;
        };
        if matches!(cursor.shape, CursorShape::Hidden) {
            return;
        }
        let position =
            origin + Vec2::new(cursor.column as f32 * cell.x, cursor.row as f32 * cell.y);
        let color = self.cursor_color();
        let shape = if self.focused {
            cursor.shape
        } else {
            CursorShape::Hollow
        };

        match shape {
            CursorShape::Block => {
                let rect = Rect::from_min_size(position, cell);
                painter.add(Shape::rect_filled(rect, 0.0, color));
                if let Some(under) = self.content.cell(cursor.column, cursor.row) {
                    let galley = painter.layout_no_wrap(
                        under.ch.to_string(),
                        self.font.font_id(),
                        self.background(),
                    );
                    if !galley.is_empty() {
                        painter.add(Shape::galley(rect.left_top(), galley, self.background()));
                    }
                }
            }
            CursorShape::Hollow => {
                painter.add(Shape::rect_stroke(
                    Rect::from_min_size(position, cell),
                    0.0,
                    Stroke::new(1.0, color),
                    egui::StrokeKind::Inside,
                ));
            }
            CursorShape::Beam => {
                painter.add(Shape::rect_filled(
                    Rect::from_min_size(position, Vec2::new(2.0, cell.y)),
                    0.0,
                    color,
                ));
            }
            CursorShape::Underline => {
                painter.add(Shape::rect_filled(
                    Rect::from_min_size(
                        position + Vec2::new(0.0, cell.y - 2.0),
                        Vec2::new(cell.x, 2.0),
                    ),
                    0.0,
                    color,
                ));
            }
            CursorShape::Hidden => {}
        }
    }
}

/// The link of a cell together with the text of its run.
fn link_at(content: &RenderableContent, column: usize, row: usize) -> Option<LinkTarget> {
    let uri = content.link_at(column, row)?.to_string();
    let id = content.cell(column, row)?.link?;
    let columns = content.columns.max(1);
    let index = row * columns + column;

    let mut start = index;
    while start > 0 && content.cells[start - 1].link == Some(id) {
        start -= 1;
    }
    let mut end = index;
    while end + 1 < content.cells.len() && content.cells[end + 1].link == Some(id) {
        end += 1;
    }

    let text: String = content.cells[start..=end]
        .iter()
        .filter(|cell| !cell.style.wide_spacer)
        .map(|cell| cell.ch)
        .collect();
    Some(LinkTarget {
        uri,
        text: text.trim().to_string(),
    })
}

/// One run of presses of the primary button, kept between frames.
#[derive(Debug, Clone, Copy)]
struct ClickRun {
    /// When the last press of the run arrived.
    time: f64,
    /// Where it arrived.
    position: egui::Pos2,
    /// How many presses the run holds, one to three.
    count: u8,
}

/// Whole lines a drag comes to, and the part of a line it leaves behind.
///
/// The part is carried to the next frame instead of being rounded away: a
/// pointer moved slowly moves by a fraction of a line a frame, and a fraction
/// rounded to nothing every time is a page that never moves. Positive is
/// towards what stood above, which is where a page pulled down goes.
fn dragged_lines(carried: f32, moved: f32, cell_height: f32) -> (i32, f32) {
    if cell_height <= 0.0 {
        return (0, carried);
    }

    let wanted = carried + moved / cell_height;
    let lines = wanted.trunc();

    (lines as i32, wanted - lines)
}

/// Whether the keys held say to move the end of the selection to the press
/// rather than to begin one there.
///
/// `Shift` and nothing else. `Ctrl` with it asks for a rectangle and takes the
/// pointer from a program that is reading it, which is a press about something
/// else, and a press cannot be both.
fn extends_selection(held: &egui::Modifiers, alt: bool) -> bool {
    held.shift && !held.ctrl && !held.command && !alt
}

/// Whether the keys held ask for a rectangle rather than a run of text.
///
/// `Ctrl` alone asks for one. `Ctrl+Shift` does not, because a caller may have
/// given that pair a meaning of its own — taking the mouse back from a program
/// that asked for it is the one this application gives it — so a selection made
/// while it is held is the ordinary kind. Adding `Alt` to it asks for a
/// rectangle again: three keys are nothing a pair is pressed by accident.
fn block_selection(held: &egui::Modifiers, alt: bool) -> bool {
    let ctrl = held.ctrl || held.command;
    ctrl && (!held.shift || alt)
}

/// Whether `Alt` is down, asked of both the state and the key.
///
/// The state of the modifiers is what a platform says the keys held *mean*, and
/// the right `Alt` does not always mean `Alt`: on a layout with a third level —
/// which is every layout that types another alphabet — it is `AltGr`, a
/// modifier of its own that the state does not report as this one. The key
/// itself is reported either way, because it is a key, so both are asked and
/// either answers.
fn alt_held(ui: &Ui, held: &egui::Modifiers) -> bool {
    held.alt
        || ui.input(|input| {
            input.key_down(egui::Key::AltLeft) || input.key_down(egui::Key::AltRight)
        })
}

/// Whether the keys held take the pointer from a program that asked for it, to
/// select a rectangle whatever that program wanted with it.
///
/// A program holding the mouse holds every button of it, and what is on the
/// screen under one — a table, a column of numbers, a log with a prefix nobody
/// wants — is still text somebody may need. `Ctrl+Shift+Alt` is the way to it
/// that does not depend on the caller having handed the mouse back first: the
/// press is not reported, it selects.
fn forced_block(held: &egui::Modifiers, alt: bool) -> bool {
    (held.ctrl || held.command) && held.shift && alt
}

/// What a drag selects, by the press it started from.
///
/// One press selects cell by cell, two select whole words and three whole
/// lines, which is how a terminal keeps what a double click found while the
/// drag grows. A rectangle is asked for by the keys held — `block_selection` —
/// and only a single press can ask for one: a rectangle of words is not a thing
/// to select.
fn drag_kind(clicks: u8, block: bool) -> SelectionKind {
    match clicks {
        2 => SelectionKind::Semantic,
        3 => SelectionKind::Lines,
        _ if block => SelectionKind::Block,
        _ => SelectionKind::Simple,
    }
}

fn grid_position(
    pointer: egui::Pos2,
    origin: egui::Pos2,
    cell: Vec2,
    size: (usize, usize),
) -> (usize, usize, bool) {
    let local = pointer - origin;
    let column_f = (local.x / cell.x).max(0.0);
    let row_f = (local.y / cell.y).max(0.0);
    let column = (column_f as usize).min(size.0.saturating_sub(1));
    let row = (row_f as usize).min(size.1.saturating_sub(1));
    let right_half = column_f.fract() >= 0.5;
    (column, row, right_half)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(keys: &[&str]) -> egui::Modifiers {
        egui::Modifiers {
            ctrl: keys.contains(&"ctrl"),
            shift: keys.contains(&"shift"),
            alt: keys.contains(&"alt"),
            command: keys.contains(&"ctrl"),
            mac_cmd: false,
        }
    }

    /// `Ctrl` asks for a rectangle and `Ctrl+Shift` does not, because a caller
    /// may have given that pair a meaning of its own; `Ctrl+Shift+Alt` asks for
    /// one again, and asks for it over a program that is holding the mouse.
    #[test]
    fn the_keys_that_ask_for_a_rectangle() {
        let alt = |keys: &[&str]| keys.contains(&"alt");

        for keys in [
            &["ctrl"][..],
            &["ctrl", "alt"][..],
            &["ctrl", "shift", "alt"][..],
        ] {
            assert!(block_selection(&held(keys), alt(keys)), "{keys:?}");
        }
        for keys in [&["ctrl", "shift"][..], &[][..], &["shift", "alt"][..]] {
            assert!(!block_selection(&held(keys), alt(keys)), "{keys:?}");
        }

        assert!(!extends_selection(&held(&["ctrl", "shift", "alt"]), true));
        assert!(!extends_selection(&held(&["ctrl", "shift"]), false));
        assert!(!extends_selection(&held(&["shift"]), true));
        assert!(extends_selection(&held(&["shift"]), false));
        assert!(!extends_selection(&held(&[]), false));

        assert!(forced_block(&held(&["ctrl", "shift", "alt"]), true));
        assert!(!forced_block(&held(&["ctrl", "alt"]), true));
        assert!(!forced_block(&held(&["ctrl", "shift"]), false));
        assert!(!forced_block(&held(&["ctrl"]), false));

        // The right `Alt` of a layout with a third level means `AltGr`, which
        // the state of the modifiers does not report as `Alt`; the key is
        // reported all the same, and it is the same three keys held.
        assert!(block_selection(&held(&["ctrl", "shift"]), true));
        assert!(forced_block(&held(&["ctrl", "shift"]), true));
    }

    /// A drag is spent in whole lines and what is left of one is carried to the
    /// next frame: a pointer moved slowly moves by a fraction of a line a
    /// frame, and a fraction rounded away every time is a page that never
    /// moves.
    #[test]
    fn a_drag_is_spent_in_whole_lines_and_carries_the_rest() {
        let cell = 20.0;

        let (lines, carried) = dragged_lines(0.0, 5.0, cell);
        assert_eq!(lines, 0, "a quarter of a line is not a line yet");

        let (lines, carried) = dragged_lines(carried, 5.0, cell);
        assert_eq!(lines, 0);
        let (lines, carried) = dragged_lines(carried, 5.0, cell);
        assert_eq!(lines, 0);
        let (lines, carried) = dragged_lines(carried, 5.0, cell);
        assert_eq!(lines, 1, "and four of them are");
        assert!(carried.abs() < 0.001, "with nothing left over");

        let (lines, _) = dragged_lines(0.0, -50.0, cell);
        assert_eq!(lines, -2, "the other way is the other way");

        let (lines, _) = dragged_lines(0.0, 5.0, 0.0);
        assert_eq!(lines, 0, "a cell of no height is a page of no lines");
    }

    /// A rectangle is asked for by a single press. Two presses are words and
    /// three are lines whatever is held, because a rectangle of words is not a
    /// thing to select.
    #[test]
    fn a_rectangle_is_asked_for_by_one_press_only() {
        assert_eq!(drag_kind(1, true), SelectionKind::Block);
        assert_eq!(drag_kind(1, false), SelectionKind::Simple);
        assert_eq!(drag_kind(2, true), SelectionKind::Semantic);
        assert_eq!(drag_kind(3, true), SelectionKind::Lines);
    }

    /// The bar of the anchor is painted only while the caller asks for it, and
    /// what it marks stands whether it is painted or not.
    #[test]
    fn the_bar_of_the_anchor_is_painted_only_when_it_is_asked_for() {
        let mut painted = Vec::new();
        for asked in [true, false] {
            let context = egui::Context::default();
            let mut terminal = Terminal::new(zyt_term::TerminalConfig {
                columns: 40,
                rows: 8,
                ..zyt_term::TerminalConfig::default()
            })
            .expect("a terminal");
            let mut content = RenderableContent::default();
            let mut cache = crate::cache::TerminalCache::new();
            terminal
                .set_selection_anchor(5, 2)
                .expect("the anchor stands on the page");

            // The first frame builds the atlas the second one is drawn from,
            // so it is the second that says what the picture is.
            let _ = painted_frame_with(&context, &mut terminal, &mut content, &mut cache, asked);
            painted.push(painted_frame_with(
                &context,
                &mut terminal,
                &mut content,
                &mut cache,
                asked,
            ));
            assert_eq!(
                content.selection_anchor,
                Some((5, 2)),
                "the place a selection would begin at is kept either way"
            );
        }

        let [shown, hidden] = painted.as_slice() else {
            unreachable!("one picture per answer")
        };
        assert!(
            shown.len() > hidden.len(),
            "the bar is a shape of its own, so the picture with it is the larger"
        );
        let bar: Vec<_> = shown.iter().filter(|part| !hidden.contains(part)).collect();
        assert!(!bar.is_empty(), "and it is what stands between the two");
    }

    /// A color a program painted over is what the cell is drawn in, and a caller
    /// that refuses it the palette gets the color of the theme back.
    ///
    /// The switch is the whole of the answer: the same bytes, the same cell,
    /// two pictures.
    #[test]
    fn a_painted_color_is_drawn_and_can_be_refused() {
        let painted = Color32::from_rgb(0xff, 0x00, 0x7f);
        let mut pictures = Vec::new();

        for allowed in [true, false] {
            let context = egui::Context::default();
            let mut terminal = Terminal::new(zyt_term::TerminalConfig {
                columns: 40,
                rows: 8,
                ..zyt_term::TerminalConfig::default()
            })
            .expect("a terminal");
            let mut content = RenderableContent::default();
            let mut cache = crate::cache::TerminalCache::new();
            terminal.feed(b"\x1b]4;1;rgb:ff/00/7f\x07\x1b[31mred");

            // The first frame builds the atlas the second one is drawn from,
            // so it is the second that says what the picture is.
            let _ = painted_colors(&context, &mut terminal, &mut content, &mut cache, allowed);
            pictures.push(painted_colors(
                &context,
                &mut terminal,
                &mut content,
                &mut cache,
                allowed,
            ));
        }

        let [with, without] = pictures.as_slice() else {
            unreachable!("one picture per answer")
        };
        assert!(with.contains(&painted), "the color the program asked for");
        assert!(
            !without.contains(&painted),
            "and nothing of it where it is refused"
        );
    }

    /// A cursor color a program asked for is what the cursor is drawn in, and a
    /// caller that refuses the palette gets the cursor color of the theme.
    #[test]
    fn a_cursor_color_is_drawn_and_can_be_refused() {
        let asked = Color32::from_rgb(0x00, 0xff, 0x00);
        let theme = TerminalTheme::dark();
        let mut pictures = Vec::new();

        for allowed in [true, false] {
            let context = egui::Context::default();
            let mut terminal = Terminal::new(zyt_term::TerminalConfig {
                columns: 40,
                rows: 8,
                ..zyt_term::TerminalConfig::default()
            })
            .expect("a terminal");
            let mut content = RenderableContent::default();
            let mut cache = crate::cache::TerminalCache::new();
            terminal.feed(b"\x1b]12;rgb:00/ff/00\x07");

            // The first frame builds the atlas the second one is drawn from,
            // so it is the second that says what the picture is.
            let _ = painted_colors(&context, &mut terminal, &mut content, &mut cache, allowed);
            pictures.push(painted_colors(
                &context,
                &mut terminal,
                &mut content,
                &mut cache,
                allowed,
            ));
        }

        let [with, without] = pictures.as_slice() else {
            unreachable!("one picture per answer")
        };
        assert!(with.contains(&asked), "the color the program asked for");
        assert!(!without.contains(&asked));
        assert!(
            without.contains(&theme.cursor),
            "and the cursor of the theme where it is refused"
        );
    }

    /// The colors one frame of the widget paints, with the palette of a program
    /// allowed or refused.
    fn painted_colors(
        context: &egui::Context,
        terminal: &mut Terminal,
        content: &mut RenderableContent,
        cache: &mut crate::cache::TerminalCache,
        program_colors: bool,
    ) -> Vec<Color32> {
        let theme = TerminalTheme::dark();
        let font = TerminalFont::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 400.0),
            )),
            ..Default::default()
        };

        let mut output = context.run_ui(input, |ui| {
            TerminalView::new(terminal, content, cache, &theme, &font)
                .auto_resize(false)
                .program_colors(program_colors)
                .show(ui);
        });
        output.textures_delta.clear();

        context
            .tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .flat_map(|primitive| match primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh.vertices,
                egui::epaint::Primitive::Callback(_) => Vec::new(),
            })
            .map(|vertex| vertex.color)
            .collect()
    }

    /// Everything one frame of the widget paints, as triangles.
    fn painted_frame(
        context: &egui::Context,
        terminal: &mut Terminal,
        content: &mut RenderableContent,
        cache: &mut crate::cache::TerminalCache,
    ) -> Vec<(egui::Pos2, egui::Pos2, Color32)> {
        painted_frame_with(context, terminal, content, cache, true)
    }

    /// The same frame, with the bar of the anchor asked for or left out.
    fn painted_frame_with(
        context: &egui::Context,
        terminal: &mut Terminal,
        content: &mut RenderableContent,
        cache: &mut crate::cache::TerminalCache,
        selection_anchor: bool,
    ) -> Vec<(egui::Pos2, egui::Pos2, Color32)> {
        let theme = TerminalTheme::dark();
        let font = TerminalFont::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 400.0),
            )),
            ..Default::default()
        };

        let mut output = context.run_ui(input, |ui| {
            TerminalView::new(terminal, content, cache, &theme, &font)
                .auto_resize(false)
                .selection_anchor(selection_anchor)
                .show(ui);
        });
        output.textures_delta.clear();

        context
            .tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .flat_map(|primitive| match primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh.vertices,
                egui::epaint::Primitive::Callback(_) => Vec::new(),
            })
            .map(|vertex| (vertex.pos, vertex.uv, vertex.color))
            .collect()
    }

    /// One frame of the widget with the events of that frame.
    fn frame_with(
        context: &egui::Context,
        terminal: &mut Terminal,
        content: &mut RenderableContent,
        cache: &mut crate::cache::TerminalCache,
        events: Vec<egui::Event>,
    ) -> TerminalOutput {
        let theme = TerminalTheme::dark();
        let font = TerminalFont::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 400.0),
            )),
            events,
            ..Default::default()
        };

        let mut reported = TerminalOutput::default();
        let mut output = context.run_ui(input, |ui| {
            let (_, out) = TerminalView::new(terminal, content, cache, &theme, &font)
                .auto_resize(false)
                .show(ui);
            reported = out;
        });
        output.textures_delta.clear();
        reported
    }

    fn frame_at(
        context: &egui::Context,
        terminal: &mut Terminal,
        content: &mut RenderableContent,
        cache: &mut crate::cache::TerminalCache,
        events: Vec<egui::Event>,
        time: f64,
    ) -> TerminalOutput {
        let theme = TerminalTheme::dark();
        let font = TerminalFont::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 400.0),
            )),
            events,
            time: Some(time),
            ..Default::default()
        };

        let mut reported = TerminalOutput::default();
        let mut output = context.run_ui(input, |ui| {
            let (_, out) = TerminalView::new(terminal, content, cache, &theme, &font)
                .auto_resize(false)
                .show(ui);
            reported = out;
        });
        output.textures_delta.clear();
        reported
    }

    fn finger(phase: egui::TouchPhase, at: egui::Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(1),
            phase,
            pos: at,
            force: None,
        }
    }

    /// A finger held still on the text selects the word under it, and lifting
    /// it hands that word to the caller.
    ///
    /// The toolkit answers a held finger where it answers a right click and
    /// takes the drag away from it, so neither of those is what this is read
    /// from: `long_touched` says a finger and the pointer says where it is.
    #[test]
    fn a_finger_held_on_the_text_selects_the_word_under_it() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 40,
            rows: 8,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut cache = crate::cache::TerminalCache::new();

        // Every cell of every row is part of one word, so a press anywhere on
        // the page is answered with the same word whatever a cell measures.
        for _ in 0..6 {
            terminal.feed(b"abcdefghijabcdefghijabcdefghijabcdefghij");
            terminal.feed(b"\r\n");
        }

        let at = egui::pos2(30.0, 20.0);
        let _ = frame_at(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            vec![egui::Event::PointerMoved(at)],
            0.0,
        );
        let _ = frame_at(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            vec![
                finger(egui::TouchPhase::Start, at),
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            0.1,
        );

        for step in 1..20 {
            let _ = frame_at(
                &context,
                &mut terminal,
                &mut content,
                &mut cache,
                vec![finger(egui::TouchPhase::Move, at)],
                0.1 + step as f64 * 0.1,
            );
            if terminal.selected_text().is_some() {
                break;
            }
        }

        assert_eq!(
            terminal.selected_text().as_deref(),
            Some("abcdefghijabcdefghijabcdefghijabcdefghij"),
            "the word under the finger is taken"
        );

        let output = frame_at(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            vec![
                finger(egui::TouchPhase::End, at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerGone,
            ],
            2.2,
        );

        assert_eq!(
            output.copied.as_deref(),
            Some("abcdefghijabcdefghijabcdefghijabcdefghij"),
            "and lifting the finger hands it over"
        );
    }

    /// A cache that was told to forget its picture reads the page again, even
    /// though the terminal says nothing changed.
    ///
    /// What it would otherwise paint from is the page the picture it threw away
    /// was built from, and whoever threw it away — the fonts were replaced, the
    /// palette changed — had no reason to touch the terminal. The grid went
    /// blank and stayed blank until a key was pressed, because a key is what
    /// made the terminal say it had changed.
    #[test]
    fn a_forgotten_picture_reads_the_page_again() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 40,
            rows: 8,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut cache = crate::cache::TerminalCache::new();

        terminal.feed(b"first");
        let _ = frame_with(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            Vec::new(),
        );
        assert!(content.rows > 0, "the first frame read the page");

        // The page moves on and somebody else reads it, which is what takes the
        // mark of having changed off the terminal. What the view holds is now
        // the page before this one, and the terminal says nothing is the matter.
        terminal.feed(b" and second");
        let mut elsewhere = RenderableContent::default();
        terminal.render_into(&mut elsewhere);
        assert!(!terminal.is_dirty());
        assert!(
            !content.cells.iter().any(|cell| cell.ch == 'd'),
            "the view has not seen the newer page"
        );

        cache.forget();
        let _ = frame_with(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            Vec::new(),
        );

        assert!(
            content.cells.iter().any(|cell| cell.ch == 'd'),
            "a picture that has to be built is built from the page the terminal holds"
        );
    }

    /// A program reading the mouse is told about the press, and the selection
    /// goes away under it just as it would with nothing reading the mouse.
    #[test]
    fn a_press_puts_the_selection_away_while_a_program_reads_the_mouse() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 40,
            rows: 8,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut cache = crate::cache::TerminalCache::new();

        terminal.feed(b"\x1b[?1000hsomething worth selecting");
        assert!(terminal.modes().mouse_report, "the program reads the mouse");
        terminal
            .selection_start(SelectionKind::Lines, 0, 0)
            .expect("a selection");
        assert!(
            terminal
                .selected_text()
                .is_some_and(|text| !text.is_empty()),
            "there is something selected to lose"
        );

        // The toolkit decides what the pointer is over from the frame before
        // it, so the pointer arrives in one frame and presses in the next.
        let at = egui::pos2(30.0, 20.0);
        let _ = frame_with(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            vec![egui::Event::PointerMoved(at)],
        );
        let output = frame_with(
            &context,
            &mut terminal,
            &mut content,
            &mut cache,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
        );

        assert!(!output.bytes.is_empty(), "the press is reported");
        assert_eq!(
            terminal.selected_text(),
            None,
            "the press put the selection away"
        );
    }

    #[test]
    fn only_a_picture_that_grew_the_atlas_asks_for_another_frame() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 40,
            rows: 8,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut cache = crate::cache::TerminalCache::new();

        terminal.feed(b"a line of plain text\r\n");
        let _ = painted_frame(&context, &mut terminal, &mut content, &mut cache);
        assert!(
            context.has_requested_repaint(),
            "the letters of the first picture grew the atlas, so the frame that sees it is asked for"
        );

        let settled = egui::Context::default();
        let mut kept = crate::cache::TerminalCache::new();
        for _ in 0..3 {
            terminal.feed(b"a line of plain text\r\n");
            let _ = painted_frame(&settled, &mut terminal, &mut content, &mut kept);
        }
        terminal.feed(b"a line of plain text\r\n");
        let built = painted_frame(&settled, &mut terminal, &mut content, &mut kept);
        assert!(!built.is_empty(), "something was painted");
        assert!(
            !settled.has_requested_repaint(),
            "a picture of letters that were already laid out grew nothing and asks for nothing"
        );

        // nothing changed, so nothing is built and nothing is asked for
        let quiet = egui::Context::default();
        let mut still = crate::cache::TerminalCache::new();
        let _ = painted_frame(&quiet, &mut terminal, &mut content, &mut still);
        let _ = painted_frame(&quiet, &mut terminal, &mut content, &mut still);
        let settled = painted_frame(&quiet, &mut terminal, &mut content, &mut still);
        assert!(
            !quiet.has_requested_repaint(),
            "a frame that painted the picture it kept asks for nothing"
        );
        assert!(!settled.is_empty(), "something was painted");
    }

    #[test]
    fn a_picture_survives_an_atlas_that_grew_under_it() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 60,
            rows: 12,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut kept = crate::cache::TerminalCache::new();

        terminal.feed(b"plain ascii to start with\r\n");
        let _ = painted_frame(&context, &mut terminal, &mut content, &mut kept);
        let first_atlas = context.fonts(|fonts| fonts.font_image_size());

        let blocks = [
            0x0400_u32, 0x0450, 0x0370, 0x03b0, 0x2500, 0x2540, 0x00c0, 0x0100,
        ];
        for block in blocks {
            for step in (0..40).step_by(40) {
                let line: String = (0..40)
                    .filter_map(|at| char::from_u32(block + step + at))
                    .collect();
                terminal.feed(line.as_bytes());
                terminal.feed(b"\r\n");
            }
            let _ = painted_frame(&context, &mut terminal, &mut content, &mut kept);
        }
        for block in [0x1f600_u32, 0x1f630, 0x1f680] {
            let line: String = (0..30)
                .filter_map(|at| char::from_u32(block + at))
                .collect();
            terminal.feed(line.as_bytes());
            terminal.feed(b"\r\n");
            let _ = painted_frame(&context, &mut terminal, &mut content, &mut kept);
        }

        let grown = context.fonts(|fonts| fonts.font_image_size());
        assert_ne!(
            grown, first_atlas,
            "the characters of this test are meant to grow the atlas"
        );

        let with_kept_glyphs = painted_frame(&context, &mut terminal, &mut content, &mut kept);
        let mut fresh = crate::cache::TerminalCache::new();
        let from_nothing = painted_frame(&context, &mut terminal, &mut content, &mut fresh);

        assert_eq!(
            with_kept_glyphs.len(),
            from_nothing.len(),
            "the same glyphs"
        );
        for (place, (kept_vertex, fresh_vertex)) in
            with_kept_glyphs.iter().zip(&from_nothing).enumerate()
        {
            assert_eq!(
                kept_vertex, fresh_vertex,
                "vertex {place} of a picture whose atlas grew under it"
            );
        }
    }

    #[test]
    fn a_picture_kept_and_built_again_is_the_one_built_from_nothing() {
        let context = egui::Context::default();
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 40,
            rows: 8,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("a terminal");
        let mut content = RenderableContent::default();
        let mut kept = crate::cache::TerminalCache::new();

        terminal.feed(b"first line\r\nsecond line\r\n\x1b[32mgreen\x1b[0m and plain\r\n");
        let _ = painted_frame(&context, &mut terminal, &mut content, &mut kept);

        terminal.feed(b"\x1b[1;1Hchanged\r\n");
        terminal.feed(b"\x1b[7mreverse\x1b[0m tail\r\n");
        let grown = painted_frame(&context, &mut terminal, &mut content, &mut kept);

        let mut fresh = crate::cache::TerminalCache::new();
        let whole = painted_frame(&context, &mut terminal, &mut content, &mut fresh);

        assert!(!whole.is_empty(), "something was painted");
        assert_eq!(
            grown.len(),
            whole.len(),
            "a picture kept row by row has the triangles of one built in full"
        );
        for (place, (kept_vertex, fresh_vertex)) in grown.iter().zip(&whole).enumerate() {
            assert_eq!(kept_vertex, fresh_vertex, "vertex {place}");
        }
    }
}
