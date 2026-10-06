//! Settings view.

use crate::app::{App, SettingsTab};
use crate::config::{LocaleSetting, ThemeMode};
use crate::sources::SourceKey;
use crate::ui::icons::{self, ADD};
use rust_i18n::t;
use std::collections::BTreeMap;

/// The two halves of the settings, and what the second one is showing.
///
/// The title of the page is the pair of them: one page says what this window is
/// like and the other what one device is like, and the device is named right
/// beside the switch that opens its half, because every control under it
/// belongs to that one device and to nothing else.
fn tabs(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new(t!("settings.title")).strong());
        ui.add_space(12.0);

        for (tab, label) in [
            (SettingsTab::General, t!("settings.general")),
            (SettingsTab::Connection, t!("settings.connection")),
        ] {
            if ui
                .selectable_label(app.settings_tab == tab, label)
                .clicked()
            {
                if tab == SettingsTab::Connection {
                    app.read_sources_again();
                }
                app.settings_tab = tab;
            }
        }

        if app.settings_tab != SettingsTab::Connection {
            return;
        }

        ui.separator();
        let shown = match app.settings_source_key() {
            Some(key) => app.source_label(&key),
            None => t!("settings.source_none").to_string(),
        };
        crate::ui::choice::row(ui, app, crate::ui::choice::Choice::SettingsSource, &shown);
        source_buttons(ui, app);
    });
}

/// Adding a console, copying the one that is shown and throwing it away.
///
/// They stand beside the picker because that is what they act on: a port is a
/// device that is there or is not, so none of the three touches one, and a
/// console is a file of this application that any of them may make or unmake.
fn source_buttons(ui: &mut egui::Ui, app: &mut App) {
    let console = app
        .settings_source_key()
        .and_then(|key| console_index(app, &key));

    if ui
        .button(ADD)
        .on_hover_text(t!("settings.console_add"))
        .clicked()
    {
        let console = crate::consoles::Console {
            name: format!("console{}", app.consoles.len() + 1),
            ..crate::consoles::default_console()
        };
        let key = console.key();
        app.consoles.push(console);
        app.save_console(app.consoles.len() - 1);
        app.settings_source = Some(key);
    }

    let Some(index) = console else {
        return;
    };
    if ui
        .button(icons::COPY)
        .on_hover_text(t!("settings.clone"))
        .clicked()
        && let Some(copied) = app.consoles.get(index).cloned()
    {
        // A copy is another console and not the same one written twice, so it
        // is given an identity of its own; what it is called is the only thing
        // it shares with what it was copied from.
        let copy = crate::consoles::Console {
            id: crate::consoles::ConsoleId::new(),
            name: format!("{} ({})", copied.name, t!("settings.copy")),
            ..copied
        };
        let key = copy.key();
        app.consoles.push(copy);
        app.save_console(app.consoles.len() - 1);
        app.settings_source = Some(key);
    }
    if ui
        .button(icons::REMOVE)
        .on_hover_text(t!("settings.remove"))
        .clicked()
    {
        app.remove_console(index);
        app.settings_source = None;
    }
}

/// Where the console of this key stands in the list, when the key names one.
fn console_index(app: &App, key: &SourceKey) -> Option<usize> {
    let id = key.console()?;
    app.consoles.iter().position(|console| console.id == id)
}

/// Draws the settings in place of the terminal.
pub fn draw(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    let mut settings = app.settings.clone();
    let mut changed = false;
    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
    crate::ui::widgets::scroll_area(ui)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            tabs(ui, app);
            ui.add_space(16.0);

            if app.settings_tab == SettingsTab::Connection {
                connection(ui, app);
                return;
            }

            changed |= appearance(ui, &mut settings, app);
            ui.add_space(24.0);
            changed |= fonts(ui, &mut settings, app);
            ui.add_space(24.0);
            changed |= performance(ui, &mut settings, app);
            ui.add_space(24.0);
            changed |= osc(ui, &mut settings, app);
            ui.add_space(24.0);
            changed |= context_menu(ui, &mut settings);
            ui.add_space(24.0);
            changed |= osc133_history(ui, &mut settings);
            ui.add_space(24.0);
            scripts(ui, app);
        });

    if changed {
        let font_changed = settings.fonts != app.settings.fonts;
        let locale_changed = settings.locale != app.settings.locale;
        let theme_changed = settings.theme != app.settings.theme;
        let osc_changed = settings.osc != app.settings.osc;
        let tooltips_changed = settings.tooltip_delay != app.settings.tooltip_delay;
        let buffer_changed = settings.read_buffer != app.settings.read_buffer;
        let lines_changed = settings.lines_interval != app.settings.lines_interval;
        let interface_size_changed =
            settings.interface_font_size != app.settings.interface_font_size;
        let themes_changed = settings.themes != app.settings.themes;
        app.settings = settings;
        app.save_settings();

        if locale_changed {
            rust_i18n::set_locale(app.settings.locale.code());
            app.registry = crate::commands::build_registry();
        }
        if theme_changed {
            app.theme.set_mode(app.settings.theme);
            context.set_visuals(app.theme.visuals());
            context.request_repaint();
        }
        if theme_changed || themes_changed {
            app.apply_terminal_theme();
        }
        if font_changed {
            app.mark_fonts_dirty();
        }
        if osc_changed {
            app.apply_osc_settings();
        }
        if tooltips_changed {
            app.apply_tooltip_delay(context);
        }
        if buffer_changed {
            app.apply_read_buffer();
        }
        if lines_changed {
            app.apply_lines_interval();
        }
        if interface_size_changed {
            app.apply_interface_size(context);
        }
    }
}

fn appearance(ui: &mut egui::Ui, settings: &mut crate::config::Settings, app: &mut App) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.appearance"));

    // Every row says one thing and is set with one control, so the names stand
    // in a column and the controls in another: rows that each begin where their
    // own label happens to end are a page nobody can read down.
    egui::Grid::new(ui.make_persistent_id("appearance"))
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(t!("settings.language"));
            ui.horizontal(|ui| {
                for (locale, key) in [
                    (LocaleSetting::System, "settings.language_system"),
                    (LocaleSetting::English, "settings.language_en"),
                    (LocaleSetting::Russian, "settings.language_ru"),
                ] {
                    if ui
                        .selectable_label(settings.locale == locale, t!(key))
                        .clicked()
                    {
                        settings.locale = locale;
                        changed = true;
                    }
                }
            });
            ui.end_row();

            ui.label(t!("settings.theme"));
            ui.horizontal(|ui| {
                for (mode, key) in [
                    (ThemeMode::System, "settings.theme_system"),
                    (ThemeMode::Dark, "settings.theme_dark"),
                    (ThemeMode::Light, "settings.theme_light"),
                ] {
                    if ui
                        .selectable_label(settings.theme == mode, t!(key))
                        .clicked()
                    {
                        settings.theme = mode;
                        changed = true;
                    }
                }
            });
            ui.end_row();

            changed |= terminal_themes(ui, settings, app);

            ui.label(t!("settings.selection_ends"))
                .on_hover_text(t!("settings.selection_ends_hint"));
            changed |= crate::ui::widgets::switch(ui, &mut settings.show_selection_ends)
                .on_hover_text(t!("settings.selection_ends_hint"))
                .changed();
            ui.end_row();

            ui.label(t!("settings.status_bar"));
            changed |= crate::ui::widgets::switch(ui, &mut settings.show_status_bar).changed();
            ui.end_row();

            ui.label(t!("settings.status_bar_height"))
                .on_hover_text(t!("settings.status_bar_height_hint"));
            changed |= ui
                .add(
                    egui::Slider::new(
                        &mut settings.status_bar_height,
                        crate::config::STATUS_BAR_HEIGHTS,
                    )
                    .step_by(1.0)
                    .suffix(t!("settings.status_bar_height_unit")),
                )
                .on_hover_text(t!("settings.status_bar_height_hint"))
                .changed();
            ui.end_row();
        });

    changed
}

/// What the window costs while it runs, and what it is allowed to spend.
///
/// They are one subject: how much of the machine this program takes — how often
/// the lines of a port are read, the memory the history is kept in, how often a
/// source is read, how long a hint waits before it is drawn — and the window that
/// says what all of it comes to. They stand apart from the look of the window
/// because they are not about how it looks.
///
/// The poll of the lines stands first because it is the one of them that costs a
/// thread of its own: every reading is a call into the driver on the thread that
/// reads the port, so it is paid whether or not anybody is watching the bar, and
/// the rest is paid by the window that is being drawn anyway.
fn performance(ui: &mut egui::Ui, settings: &mut crate::config::Settings, app: &mut App) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.performance"));

    egui::Grid::new(ui.make_persistent_id("performance"))
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(t!("settings.lines_interval"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut settings.lines_interval)
                        .range(crate::config::LINES_INTERVAL_MS)
                        .suffix(t!("format.milliseconds")),
                )
                .on_hover_text(t!("settings.lines_interval_hint"))
                .changed();
            ui.end_row();

            let lines = crate::config::scrollback_lines(
                settings.scrollback_memory,
                app.session.terminal.size().0,
            );
            ui.label(t!("settings.scrollback"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut settings.scrollback_memory)
                            .range(1..=4096)
                            .suffix(t!(crate::format::SIZE_UNITS[2])),
                    )
                    .on_hover_text(t!("settings.scrollback_hint", lines = lines))
                    .changed();
                ui.label(
                    egui::RichText::new(t!("settings.scrollback_lines", lines = lines))
                        .weak()
                        .italics(),
                );
            });
            ui.end_row();

            ui.label(t!("settings.read_buffer"));
            ui.horizontal(|ui| {
                let mut limited = settings.read_buffer.is_some();
                if crate::ui::widgets::switch(ui, &mut limited)
                    .on_hover_text(t!("settings.read_buffer_hint"))
                    .changed()
                {
                    settings.read_buffer = limited.then_some(crate::config::DEFAULT_READ_BUFFER);
                    changed = true;
                }
                if let Some(size) = settings.read_buffer.as_mut() {
                    changed |= ui
                        .add(
                            egui::DragValue::new(size)
                                .range(crate::config::READ_BUFFER_SIZES)
                                .suffix(t!(crate::format::SIZE_UNITS[1])),
                        )
                        .on_hover_text(t!("settings.read_buffer_hint"))
                        .changed();
                }
            });
            ui.end_row();

            ui.label(t!("settings.tooltip_delay"));
            ui.horizontal(|ui| {
                let mut delayed = settings.tooltip_delay.is_some();
                if crate::ui::widgets::switch(ui, &mut delayed)
                    .on_hover_text(t!("settings.tooltip_delay_hint"))
                    .changed()
                {
                    settings.tooltip_delay =
                        delayed.then_some(crate::config::DEFAULT_TOOLTIP_DELAY);
                    changed = true;
                }
                if let Some(delay) = settings.tooltip_delay.as_mut() {
                    changed |= ui
                        .add(
                            egui::DragValue::new(delay)
                                .speed(0.05)
                                .range(0.1..=3.0)
                                .max_decimals(2)
                                .suffix(t!("format.seconds")),
                        )
                        .on_hover_text(t!("settings.tooltip_delay_hint"))
                        .changed();
                }
            });
            ui.end_row();

            ui.label(t!("settings.debug_window"))
                .on_hover_text(t!("settings.debug_window_hint"));
            changed |= crate::ui::widgets::switch(ui, &mut settings.show_debug_window)
                .on_hover_text(t!("settings.debug_window_hint"))
                .changed();
            ui.end_row();
        });

    changed |= read_steps(ui, settings, app);
    changed
}

/// The ladder that says how long the bytes of a source wait, by how fast it is
/// talking.
///
/// It stands last in the block because it is the one setting here that is a
/// table rather than a value, and because it is read against what the window of
/// numbers shows: the speed of the source and the wait in force.
///
/// A row holds up to its speed, and the row in force is marked while a source is
/// connected. The last row is the one past every step: what the wait grows to
/// when a source is faster than the ladder goes.
///
/// The rows stay where they were put. A ladder that sorted itself as the speeds
/// were typed moved the row under the hand that was typing it, and a value it cut
/// to fit was the page deciding which of two numbers was the mistake. Instead a
/// row that does not read as a step of a ladder wears a warning
/// (`config::read_step_amiss`, `config::read_above_amiss`) and nothing else
/// happens: the reader is the one who knows what was meant, and the ladder works
/// whatever order it is written in — `config::read_pace` asks for the slowest step
/// the source has not passed and not for the first one.
fn read_steps(ui: &mut egui::Ui, settings: &mut crate::config::Settings, app: &mut App) -> bool {
    let mut changed = false;
    let speed = app.session.byte_rate();
    let pace = crate::config::read_pace(&settings.read_steps, speed);
    let connected = app.session.has_source();

    ui.add_space(8.0);
    ui.label(t!("settings.read_steps"))
        .on_hover_text(t!("settings.read_steps_hint"));

    let mut remove = None;
    egui::Grid::new(ui.make_persistent_id("read_steps"))
        .num_columns(4)
        .spacing([16.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t!("settings.read_steps_speed"))
                    .weak()
                    .italics(),
            );
            ui.label(
                egui::RichText::new(t!("settings.read_steps_interval"))
                    .weak()
                    .italics(),
            );
            ui.label("");
            ui.label("");
            ui.end_row();

            for index in 0..settings.read_steps.len() {
                let step = settings.read_steps[index];
                let marked = connected && pace == crate::config::ReadPace::Step(step);

                ui.horizontal(|ui| {
                    ui.label(if marked { icons::CURRENT } else { " " });
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut settings.read_steps[index].speed)
                                .range(crate::config::READ_STEP_SPEEDS)
                                .suffix(crate::format::rate_unit()),
                        )
                        .on_hover_text(t!("settings.read_steps_speed_hint"))
                        .changed();
                });

                ui.horizontal(|ui| {
                    let mut waits = step.interval.is_some();
                    if crate::ui::widgets::switch(ui, &mut waits)
                        .on_hover_text(t!("settings.read_steps_interval_hint"))
                        .changed()
                    {
                        settings.read_steps[index].interval = waits.then_some(27);
                        changed = true;
                    }
                    match settings.read_steps[index].interval.as_mut() {
                        Some(wait) => {
                            changed |= ui
                                .add(
                                    egui::DragValue::new(wait)
                                        .range(crate::config::READ_STEP_INTERVALS)
                                        .suffix(t!("format.milliseconds")),
                                )
                                .on_hover_text(t!("settings.read_steps_interval_hint"))
                                .changed();
                        }
                        None => {
                            ui.label(
                                egui::RichText::new(t!("settings.read_steps_at_once"))
                                    .weak()
                                    .italics(),
                            );
                        }
                    }
                });

                if ui
                    .button(icons::REMOVE)
                    .on_hover_text(t!("settings.read_steps_remove"))
                    .clicked()
                {
                    remove = Some(index);
                }
                amiss(
                    ui,
                    crate::config::read_step_amiss(&settings.read_steps, index),
                );
                ui.end_row();
            }

            let marked = connected && pace == crate::config::ReadPace::Above;
            ui.horizontal(|ui| {
                ui.label(if marked { icons::CURRENT } else { " " });
                ui.label(t!("settings.read_steps_above"))
                    .on_hover_text(t!("settings.read_steps_above_hint"));
            });

            ui.horizontal(|ui| {
                let mut waits = settings.read_above.interval.is_some();
                if crate::ui::widgets::switch(ui, &mut waits)
                    .on_hover_text(t!("settings.read_steps_above_hint"))
                    .changed()
                {
                    settings.read_above.interval = waits.then_some(1000);
                    changed = true;
                }
                match settings.read_above.interval.as_mut() {
                    Some(wait) => {
                        changed |= ui
                            .add(
                                egui::DragValue::new(wait)
                                    .range(crate::config::READ_STEP_INTERVALS)
                                    .suffix(t!("format.milliseconds")),
                            )
                            .on_hover_text(t!("settings.read_steps_above_hint"))
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut settings.read_above.linear,
                                t!("settings.read_steps_linear"),
                            )
                            .on_hover_text(t!("settings.read_steps_linear_hint"))
                            .changed();
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(t!("settings.read_steps_at_once"))
                                .weak()
                                .italics(),
                        );
                    }
                }
            });

            ui.label("");
            amiss(
                ui,
                crate::config::read_above_amiss(&settings.read_steps, settings.read_above),
            );
            ui.end_row();
        });

    if let Some(index) = remove {
        settings.read_steps.remove(index);
        changed = true;
    }

    if settings.read_steps.len() < crate::config::READ_STEPS_MAX
        && ui
            .button(ADD)
            .on_hover_text(t!("settings.read_steps_add"))
            .clicked()
    {
        let fastest = settings
            .read_steps
            .iter()
            .map(|step| step.speed)
            .max()
            .unwrap_or(0);
        settings.read_steps.push(crate::config::ReadStep {
            speed: fastest.saturating_mul(4).max(1),
            interval: Some(60),
        });
        changed = true;
    }

    changed
}

/// The mark of a row of the ladder that does not read as a step of one.
///
/// It says that and no more. What is wrong with such a row is the two numbers on
/// it and the two on the row above, all four of them already on the screen, and a
/// sentence spelling out which pair disagrees is a sentence saying what the reader
/// is looking at.
///
/// A row that reads as it should keeps the room the mark would take, so a ladder
/// does not shuffle as the numbers on it are typed.
fn amiss(ui: &mut egui::Ui, amiss: bool) {
    let mark = match amiss {
        true => egui::RichText::new(icons::WARNING).color(ui.visuals().error_fg_color),
        false => egui::RichText::new(icons::WARNING).color(egui::Color32::TRANSPARENT),
    };
    ui.label(mark);
}

/// The palette the terminal wears in each color mode.
///
/// The interface itself keeps the colors of the toolkit; only the terminal
/// follows these two.
///
/// It fills two rows of the grid it is called in and ends both of them: the
/// palettes on the first, and under them the line saying where the files they
/// are read from lie. That line stands in the column of the values and not in
/// the one of the names, because it is about the two palettes beside it.
fn terminal_themes(
    ui: &mut egui::Ui,
    settings: &mut crate::config::Settings,
    app: &mut App,
) -> bool {
    let mut reload = false;

    ui.label(t!("settings.terminal_theme"))
        .on_hover_text(t!("settings.terminal_theme_hint"));
    ui.horizontal(|ui| {
        for (dark, label) in [
            (true, "settings.theme_dark"),
            (false, "settings.theme_light"),
        ] {
            ui.label(egui::RichText::new(t!(label)).weak());
            let shown = settings.themes.name(dark).to_string();
            crate::ui::choice::row(
                ui,
                app,
                crate::ui::choice::Choice::TerminalTheme { dark },
                &shown,
            );
        }
        reload = ui
            .button(icons::REFRESH)
            .on_hover_text(t!("settings.themes_reload"))
            .clicked();
    });
    ui.end_row();

    ui.label("");
    ui.label(
        egui::RichText::new(t!(
            "settings.theme_files",
            directory = crate::themes::THEMES_DIR
        ))
        .weak()
        .italics(),
    );
    ui.end_row();

    if reload {
        app.reload_themes();
    }

    false
}

/// The families the window is drawn with, and how big the terminal is.
///
/// Nothing is installed here: the families are the ones the system already
/// has, so the lists are what the font database of the platform reports of
/// them.
///
/// The terminal is a chain and the interface one family. A terminal shows
/// whatever a device sends, and one family rarely carries all of it, so the
/// families collected here are asked in turn and the toolkit's own fonts stand
/// behind them. The interface is prose of this program and is read in one font.
///
/// All of it stands in one grid, the chain included: the name of a family, the
/// two buttons that move it and the one that throws it away each keep a column
/// of their own, so a chain of families named at every length is still a block
/// with its buttons in a straight line.
fn fonts(ui: &mut egui::Ui, settings: &mut crate::config::Settings, app: &mut App) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.font"));

    egui::Grid::new(ui.make_persistent_id("fonts"))
        .num_columns(4)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(t!("settings.font_size"))
                .on_hover_text(t!("settings.font_size_hint"));
            changed |= ui
                .add(egui::Slider::new(
                    &mut settings.font_size,
                    crate::config::FONT_SIZES,
                ))
                .changed();
            ui.end_row();

            changed |= terminal_chain(ui, app, &mut settings.fonts.terminal);

            ui.label(t!("settings.font_size_interface"))
                .on_hover_text(t!("settings.font_size_interface_hint"));
            changed |= ui
                .add(egui::Slider::new(
                    &mut settings.interface_font_size,
                    crate::config::FONT_SIZES,
                ))
                .changed();
            ui.end_row();

            interface_choice(ui, app);
            ui.end_row();
        });

    changed
}

/// The families the terminal is drawn with, one row each, and the button that
/// collects one more.
///
/// It fills as many rows of the grid of the fonts as the chain is long, plus
/// the one the button stands on, and ends each of them.
///
/// The order is the order they are asked in, so it is the order they stand in
/// and each row carries the two buttons that move it. They stand side by side
/// in one cell: up and down are the two halves of one question — where among
/// the others this family belongs — and a pair split over two columns is a pair
/// that has to be aimed at twice.
///
/// The name of the setting stands on the first of those rows rather than beside
/// the middle of them: what it names is the block, and a word floating level
/// with the third family of a chain looks like a word about that family.
///
/// A row is the name that was collected and nothing else. Whether the machine
/// still has that family, and whether all its glyphs are one cell wide, are
/// questions about every font of the machine, and this block is drawn every
/// frame the settings stand: what it shows is what was chosen, and choosing is
/// where the fonts of the machine are looked at.
///
/// Nothing collected is the font of the toolkit, which is what the terminal was
/// drawn in before anybody collected anything, and the block says so in that
/// one row rather than standing empty.
fn terminal_chain(ui: &mut egui::Ui, app: &mut App, chain: &mut Vec<String>) -> bool {
    let mut moved: Option<(usize, usize)> = None;
    let mut removed: Option<usize> = None;
    let mut changed = false;

    ui.label(t!("settings.font_terminal"))
        .on_hover_text(t!("settings.font_terminal_hint"));
    if chain.is_empty() {
        ui.label(egui::RichText::new(t!("settings.font_builtin")).weak());
        ui.end_row();
    }

    for (index, name) in chain.iter().enumerate() {
        if index > 0 {
            ui.label("");
        }
        ui.label(name.as_str());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(index > 0, egui::Button::new(icons::UP))
                .on_hover_text(t!("settings.font_up"))
                .clicked()
            {
                moved = Some((index, index - 1));
            }
            if ui
                .add_enabled(index + 1 < chain.len(), egui::Button::new(icons::DOWN))
                .on_hover_text(t!("settings.font_down"))
                .clicked()
            {
                moved = Some((index, index + 1));
            }
        });
        if ui
            .button(icons::REMOVE)
            .on_hover_text(t!("settings.remove"))
            .clicked()
        {
            removed = Some(index);
        }
        ui.end_row();
    }

    ui.label("");
    crate::ui::choice::row(
        ui,
        app,
        crate::ui::choice::Choice::FontAdd,
        &format!("{ADD} {}", t!("settings.font_add")),
    );
    ui.end_row();

    if let Some((from, to)) = moved {
        chain.swap(from, to);
        changed = true;
    }
    if let Some(index) = removed
        && index < chain.len()
    {
        chain.remove(index);
        changed = true;
    }
    changed
}

/// The two cells naming the family the interface is drawn in: the name of the
/// part and, beside it, the family.
///
/// It fills a row of the grid of the fonts and ends no row itself, so the
/// caller decides what else stands on that row.
///
/// What it shows is the family in use, or the name of the built-in font when
/// none was chosen; the list itself is the menu the row opens. A family the
/// system no longer has is still named here while it is the chosen one, instead
/// of quietly becoming something else.
fn interface_choice(ui: &mut egui::Ui, app: &mut App) {
    let shown = app
        .settings
        .fonts
        .interface
        .clone()
        .unwrap_or_else(|| t!("settings.font_builtin").to_string());

    ui.label(t!("settings.font_interface"))
        .on_hover_text(t!("settings.font_interface_hint"));
    crate::ui::choice::row(ui, app, crate::ui::choice::Choice::Font, &shown);
}

/// Everything that belongs to one port or console.
///
/// Which one is the picker of the title bar, and it follows the connection of
/// the window until somebody picks another: the settings of a device outlive
/// the device, so a board that is unplugged is configured the same way as one
/// that is on the line.
fn connection(ui: &mut egui::Ui, app: &mut App) {
    let Some(key) = app.settings_source_key() else {
        ui.label(t!("settings.source_none_hint"));
        return;
    };

    match (console_index(app, &key), key.port()) {
        (Some(index), _) => console_fields(ui, app, index),
        (None, Some(id)) => {
            let id = id.clone();
            shown_lines(ui, app, &id);
            ui.add_space(24.0);
            line_flags(ui, app, &id);
            ui.add_space(24.0);
            baud_rates(ui, app, &id);
        }
        (None, None) => {}
    }
    ui.add_space(24.0);
    variables(ui, app, &key);
    ui.add_space(24.0);
    offered_scripts(ui, app, &key);
}

/// What one console is: the program it runs, where it starts, whether its
/// output may be acted on and whether it comes back when it ends.
///
/// The first three are a value each and stand in a grid together. The last two
/// are a yes or a no, and they stand under it, each beside the sentence that
/// says what it does — a switch whose meaning is only on the pointer is a
/// switch that has to be hunted for to be read, and these two are the ones
/// somebody comes to this page unsure about.
///
/// A port has none of this — it is a device that is there or is not — which is
/// why the two halves of this page are not the same page.
fn console_fields(ui: &mut egui::Ui, app: &mut App, index: usize) {
    let name_label = t!("settings.console_name").to_string();
    let program_label = t!("settings.console_program").to_string();
    let directory_label = t!("settings.console_directory").to_string();
    let width =
        crate::ui::widgets::label_width(ui, &[&name_label, &program_label, &directory_label]);

    let values = app
        .consoles
        .get(index)
        .map(|console| console.memory.variable_map())
        .unwrap_or_default();
    let shell = zyt_pty::default_shell();
    let field = crate::ui::widgets::monospace_width(ui, CONSOLE_FIELD);

    let App {
        consoles,
        ui: state,
        ..
    } = app;
    let Some(console) = consoles.get_mut(index) else {
        return;
    };
    let mut changed = false;
    let mut pick_directory = false;
    let row = ui.make_persistent_id(("console", index));

    egui::Grid::new(row)
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Label::new(&name_label),
            );
            let shown = crate::sources::expand(&console.name, &values);
            ui.horizontal(|ui| {
                let id = row.with("name");
                changed |= crate::ui::widgets::pencil(ui, &mut state.editing, id, &console.name);
                changed |= crate::ui::widgets::editable(
                    ui,
                    &mut state.editing,
                    id,
                    &mut console.name,
                    &shown,
                    &name_label,
                    field,
                );
            });
            ui.end_row();

            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Label::new(&program_label),
            );
            let shown = match console.program.trim().is_empty() {
                true => shell.clone(),
                false => crate::sources::expand(&console.program, &values),
            };
            ui.horizontal(|ui| {
                let id = row.with("program");
                changed |= crate::ui::widgets::pencil(ui, &mut state.editing, id, &console.program);
                changed |= crate::ui::widgets::editable(
                    ui,
                    &mut state.editing,
                    id,
                    &mut console.program,
                    &shown,
                    &shell,
                    field,
                );
            });
            ui.end_row();

            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Label::new(&directory_label),
            );
            let shown = crate::sources::expand(&console.directory, &values);
            ui.horizontal(|ui| {
                let id = row.with("directory");
                changed |=
                    crate::ui::widgets::pencil(ui, &mut state.editing, id, &console.directory);
                if ui
                    .button(icons::FOLDER)
                    .on_hover_text(t!("settings.console_directory_pick"))
                    .clicked()
                {
                    pick_directory = true;
                }
                changed |= crate::ui::widgets::editable(
                    ui,
                    &mut state.editing,
                    id,
                    &mut console.directory,
                    &shown,
                    &t!("settings.console_directory_hint"),
                    field,
                );
            });
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let icon = if console.trusted {
            icons::TRUSTED
        } else {
            icons::UNTRUSTED
        };
        if ui
            .selectable_label(
                console.trusted,
                egui::RichText::new(icon).color(icons::trust_color(ui, console.trusted)),
            )
            .clicked()
        {
            console.trusted = !console.trusted;
            changed = true;
        }
        ui.label(t!("settings.osc_trusted"));
    });
    ui.horizontal(|ui| {
        if ui
            .selectable_label(console.restart, icons::RESTART)
            .clicked()
        {
            console.restart = !console.restart;
            changed = true;
        }
        ui.label(t!("settings.console_restart_hint"));
    });

    // A rename leaves the pick where it is: what the page is showing is the
    // console, and the console is what it was before somebody typed into its
    // name.
    if changed {
        app.save_console(index);
    }
    if pick_directory {
        app.pick_console_directory(index);
    }
}

/// Which of the lines this device shows in the status bar.
///
/// It is of the device and not of the window because it is a fact about the
/// device: which of its letters ever move is a thing about the adapter and the
/// board at the end of it, and the answer for one is no answer for the next. A
/// `DCD` tied high by the adapter and an `RI` wired to nothing are two letters of
/// noise in a row read at a glance, which is why two of the eight begin switched
/// off.
///
/// The plate of the signals draws the same set, which is what the heading says.
/// The row of letters and the tracks beside them are the same signals read two
/// ways, so switching one off is one decision and not two that can disagree.
///
/// One line to a row, the switch beside the sentence that says what it is. Eight
/// letters in a wrapped row were eight switches of three letters each and nothing
/// saying which line any of them was, so the page could be read by whoever wrote
/// it and by nobody else.
fn shown_lines(ui: &mut egui::Ui, app: &mut App, id: &zyt_serial::PortId) {
    use crate::config::StatusLine;

    ui.label(egui::RichText::new(t!("settings.shown_lines")).strong());

    let mut lines = app
        .ports_memory
        .get(id)
        .map(|memory| memory.shown_lines)
        .unwrap_or_default();
    let mut changed = false;

    ui.add_space(8.0);
    egui::Grid::new(ui.make_persistent_id("shown_lines"))
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for line in StatusLine::ALL {
                let standing = lines.shows(line);
                if ui.selectable_label(standing, line.label()).clicked() {
                    *lines.shown_mut(line) = !standing;
                    changed = true;
                }
                ui.label(t!(line.note_key()));
                ui.end_row();
            }
        });

    if !changed {
        return;
    }
    app.port_memory_mut(id).shown_lines = lines;
    app.save_memory(&SourceKey::Port(id.clone()));
}

/// The two things a line is opened with that are a yes or a no.
///
/// Both are of the device and not of the window, so they stand on this page and
/// are written into the file of that device: the speed of a line is chosen in
/// the status bar, where it is read, and these two are set once for a board and
/// left alone.
///
/// They are switches beside the sentence that says what each of them does,
/// rather than a word the pointer has to be rested on: what they decide happens
/// at the two moments nobody is watching — when the port opens and when it
/// closes — so the page is where they have to be readable.
fn line_flags(ui: &mut egui::Ui, app: &mut App, id: &zyt_serial::PortId) {
    ui.label(egui::RichText::new(t!("settings.line_flags")).strong());

    let mut params = app
        .ports_memory
        .get(id)
        .map(|memory| memory.line)
        .unwrap_or(app.settings.line);
    let mut changed = false;

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        changed |= crate::ui::widgets::switch(ui, &mut params.flush_on_open)
            .on_hover_text(t!("settings.flush_on_open_hint"))
            .changed();
        ui.label(t!("settings.flush_on_open"));
    });
    if zyt_serial::HUPCL_SUPPORTED {
        ui.horizontal(|ui| {
            changed |= crate::ui::widgets::switch(ui, &mut params.hupcl).changed();
            ui.label(t!("settings.hupcl"));
        });
    }

    if !changed {
        return;
    }
    // The device the page is showing is not always the device the window is on,
    // so the file is written either way and the open port is told only when the
    // two are the same one.
    app.port_memory_mut(id).line = params;
    app.save_memory(&SourceKey::Port(id.clone()));
    if app.active_port_id().as_ref() == Some(id) {
        app.set_line_params(params);
    }
}

/// The speeds the menu of the line offers for this device.
///
/// Only a port has one: a console is a program of this machine and there is no
/// line under it to run at any speed at all. Left empty it is the shared list
/// of the settings, so a device is given a list of its own only when it wants
/// one nobody else does.
fn baud_rates(ui: &mut egui::Ui, app: &mut App, id: &zyt_serial::PortId) {
    ui.label(egui::RichText::new(t!("settings.baud_rates")).strong());

    let own = app
        .ports_memory
        .get(id)
        .map(|memory| memory.baud_rates.clone())
        .unwrap_or_default();
    let mut text = own
        .iter()
        .map(u32::to_string)
        .collect::<Vec<String>>()
        .join(" ");
    let shared = app
        .settings
        .baud_rates
        .iter()
        .map(u32::to_string)
        .collect::<Vec<String>>()
        .join(" ");

    let changed = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(shared)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        )
        .on_hover_text(t!("settings.baud_rates_hint"))
        .changed();
    if !changed {
        return;
    }

    let rates: Vec<u32> = text
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    app.port_memory_mut(id).baud_rates = rates;
    app.save_memory(&SourceKey::Port(id.clone()));
}

/// How many characters of a line of a console stand in its field.
///
/// A program line and a path are both longer than a glance, and a field that
/// shows half of one is a field that is scrolled to be read.
const CONSOLE_FIELD: usize = 64;

/// The widest a name is ever written in.
///
/// A name is letters, digits, the hyphen and the underscore, and the ones a
/// profile asks for are words: `remote_host` is the longest this program ships.
/// There is nothing to be gained by a field wider than the names that go in it,
/// and what it gives up goes to the value beside it.
const VARIABLE_NAME: usize = 24;

/// The narrowest, which is what an empty field is: its hint and no more.
const VARIABLE_NAME_LEAST: usize = 12;

/// The least a value is ever offered, whatever is left of the page.
///
/// A host, a user and a path are all longer than a glance, and a field showing
/// half of one is a field that is scrolled to be read, so a page too narrow to
/// give a value this much gives it this much anyway.
const VARIABLE_VALUE: usize = 32;

/// The narrowest a value field is, which is what an empty one shows its hint
/// in. It grows from here with what is typed into it.
const VARIABLE_VALUE_LEAST: usize = 16;

/// What is left of the width of the page for a value: everything the name, the
/// two buttons, what asks for it and the gaps between them do not take.
///
/// A value is the long half of the pair — an address, a path, a user at a host
/// — so it is given the room rather than a width of its own, and the page is as
/// wide as the window. A page too narrow to give it that much gives it that
/// much anyway and is scrolled sideways, which is what a window nobody widened
/// asks for.
fn value_width(ui: &egui::Ui, name: f32, asked: f32) -> f32 {
    let button = ui.spacing().interact_size.y + ui.spacing().button_padding.x * 2.0;
    let gaps = ui.spacing().item_spacing.x * 5.0;
    (ui.available_width() - name - asked - button * 2.0 - gaps)
        .max(crate::ui::widgets::monospace_width(ui, VARIABLE_VALUE))
}

/// Room the column saying what asks for a value keeps.
///
/// It is as wide as the longest of those lines and never wider than a third of
/// the page: what asks for a value is worth a glance, and the value itself is
/// what somebody came to this page to write.
fn asked_width(ui: &egui::Ui, asked: &BTreeMap<String, Vec<String>>) -> f32 {
    if asked.is_empty() {
        return 0.0;
    }

    let lines: Vec<String> = asked.values().map(|who| who.join(", ")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    crate::ui::widgets::label_width(ui, &lines).min(ui.available_width() / 3.0)
}

/// How wide the two columns of the variables stand: as wide as the longest
/// name and the longest value written in them.
///
/// Both are measured over the whole list and not per row, because they are the
/// columns of a grid: a field that fits its own text and stops short of the
/// cell it stands in would leave the column ragged and the buttons beside it
/// walking left and right from row to row.
///
/// The name is measured first, because what is left of the page once the names,
/// the two buttons and the gaps have taken theirs is what the value may grow
/// into.
fn variable_widths(
    ui: &egui::Ui,
    app: &App,
    key: &SourceKey,
    asked: f32,
    missing: &[String],
) -> (f32, f32) {
    let written =
        |text: &str, least, most| crate::ui::widgets::written_width(ui, text, least, most);
    let kept: &[crate::sources::SourceVariable] = app
        .memory(key)
        .map(|memory| memory.variables.as_slice())
        .unwrap_or_default();

    let name_most = crate::ui::widgets::monospace_width(ui, VARIABLE_NAME);
    let name = kept
        .iter()
        .map(|variable| written(&variable.name, VARIABLE_NAME_LEAST, name_most))
        .chain(
            missing
                .iter()
                .map(|name| crate::ui::widgets::button_width(ui, &format!("{ADD} {name}"))),
        )
        .fold(written("", VARIABLE_NAME_LEAST, name_most), f32::max);

    let value_most = value_width(ui, name, asked);
    let value = kept
        .iter()
        .map(|variable| written(&variable.value, VARIABLE_VALUE_LEAST, value_most))
        .fold(written("", VARIABLE_VALUE_LEAST, value_most), f32::max);

    (name, value)
}

/// The values this source answers a command line with, by name.
///
/// One row is one name: the name itself, what it answers with, the two buttons
/// that copy and throw the row away, and what asks for that name. A name that
/// is asked for and has no row yet is a row as well — a button in the column of
/// the names, and in the column of the askers what is waiting for it. It stands
/// in the same grid and not in a list of its own under it, because it is the
/// same thing said once: one line per name, whether this connection answers it
/// or not.
fn variables(ui: &mut egui::Ui, app: &mut App, key: &SourceKey) {
    let mut changed = false;
    let mut remove = None;
    let mut clone = None;
    let mut wanted = None;
    let mut add = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t!("settings.variables")).strong());
        add = ui
            .button(ADD)
            .on_hover_text(t!("settings.variables_hint"))
            .clicked();
    });

    let asked = asked_by_name(app, key);
    let missing = missing_rows(app, key, &asked);
    let asked_room = asked_width(ui, &asked);
    let (name_width, value_width) = variable_widths(ui, app, key, asked_room, &missing);
    let height = ui.spacing().interact_size.y;
    let grid = ui.make_persistent_id(("variables", key));
    egui::Grid::new(grid)
        .num_columns(5)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            let Some(variables) = app.memory_mut(key).map(|memory| &mut memory.variables) else {
                return;
            };
            for (index, variable) in variables.iter_mut().enumerate() {
                let written = crate::ui::widgets::sized_field(
                    ui,
                    name_width,
                    egui::TextEdit::singleline(&mut variable.name)
                        .id(grid.with(("name", index)))
                        .hint_text("remote_host")
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text(t!("settings.variable_name_hint"))
                .changed();
                if written {
                    variable
                        .name
                        .retain(|point| crate::sources::is_variable_name(&point.to_string()));
                    changed = true;
                }
                changed |= crate::ui::widgets::sized_field(
                    ui,
                    value_width,
                    egui::TextEdit::singleline(&mut variable.value)
                        .id(grid.with(("value", index)))
                        .hint_text("192.168.1.1")
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text(t!("settings.variable_value_hint"))
                .changed();
                if ui
                    .button(icons::COPY)
                    .on_hover_text(t!("settings.clone"))
                    .clicked()
                {
                    clone = Some(index);
                }
                if ui
                    .button(icons::REMOVE)
                    .on_hover_text(t!("settings.remove"))
                    .clicked()
                {
                    remove = Some(index);
                }
                asked_cell(ui, &asked, &variable.name, asked_room);
                ui.end_row();
            }

            for name in &missing {
                let who = asked.get(name).cloned().unwrap_or_default().join(", ");
                if ui
                    .add_sized(
                        [name_width, height],
                        egui::Button::new(format!("{ADD} {name}")),
                    )
                    .on_hover_text(t!("settings.variable_add", name = name, who = who))
                    .clicked()
                {
                    wanted = Some(name.clone());
                }
                ui.label("");
                ui.label("");
                ui.label("");
                asked_cell(ui, &asked, name, asked_room);
                ui.end_row();
            }
        });

    let Some(variables) = app.memory_mut(key).map(|memory| &mut memory.variables) else {
        return;
    };
    if add {
        variables.push(crate::sources::SourceVariable::default());
        changed = true;
    }
    if let Some(name) = wanted {
        variables.push(crate::sources::SourceVariable {
            name,
            value: String::new(),
        });
        changed = true;
    }
    if let Some(index) = clone
        && let Some(copy) = variables.get(index).cloned()
    {
        variables.insert(index + 1, copy);
        changed = true;
    }
    if let Some(index) = remove
        && index < variables.len()
    {
        variables.remove(index);
        changed = true;
    }
    if changed {
        app.save_memory(key);
    }
}

/// The names that are asked for and have no row of their own yet.
///
/// A name with a row is not here whether the row answers with anything or not:
/// an empty value is an answer somebody is in the middle of writing, and a
/// second row of that name would answer nothing twice.
fn missing_rows(app: &App, key: &SourceKey, asked: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let answered: Vec<&str> = app
        .memory(key)
        .map(|memory| {
            memory
                .variables
                .iter()
                .map(|variable| variable.name.as_str())
                .collect()
        })
        .unwrap_or_default();

    asked
        .keys()
        .filter(|name| !answered.contains(&name.as_str()))
        .cloned()
        .collect()
}

/// What asks this source for a value, by the name it asks for.
///
/// The console of the source asks through its own line and directory; every
/// transfer profile it offers asks through its command lines. What asks is
/// kept beside the name it wants, because a bare list of names says what is
/// missing and not what would stop working without it — and the name is what
/// the row of a value is found by, so this is keyed the way the rows are read.
fn asked_by_name(app: &App, key: &SourceKey) -> BTreeMap<String, Vec<String>> {
    let mut asked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ask = |who: String, names: Vec<String>| {
        for name in names {
            asked.entry(name).or_default().push(who.clone());
        }
    };

    if let Some(console) = console_index(app, key).and_then(|index| app.consoles.get(index)) {
        ask(console.name.clone(), console.variables());
    }
    for entry in app.ticked_scripts_of(Some(key)) {
        ask(entry.id.clone(), entry.manifest.variables.clone());
    }
    asked
}

/// The names of that list this source cannot answer.
///
/// What it was answered on the way in counts as an answer: the window that
/// asks before a console is opened is where a value that is not a decision
/// about the source is given, and a warning about a value that stands in the
/// connection would be a warning about nothing.
fn unanswered(app: &App, key: &SourceKey, names: &[String]) -> Vec<String> {
    let answered = app.variables_of(Some(key));

    names
        .iter()
        .filter(|name| !answered.contains_key(*name))
        .cloned()
        .collect()
}

/// The cell right of one variable: what asks for it, or nothing when nothing
/// does.
///
/// It stands in the row of the value it is about rather than in a list of its
/// own under the grid: what wants a value is something to know while that value
/// is being written, and a list somewhere else has to be matched back to the
/// rows by name before it says anything. It is cut to the width of its column
/// and says the whole of itself on the pointer, because it is prose of
/// whatever length the names of the profiles happen to be.
///
/// It is held to the room the column was measured for, so a profile with a long
/// name cuts its own line instead of pushing the value beside it off the page.
fn asked_cell(ui: &mut egui::Ui, asked: &BTreeMap<String, Vec<String>>, name: &str, room: f32) {
    let Some(who) = asked.get(name) else {
        ui.label("");
        return;
    };

    let line = who.join(", ");
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.add(egui::Label::new(egui::RichText::new(&line).weak()).truncate())
            .on_hover_text(t!("settings.variable_asked_by", who = line));
    });
}

/// Which scripts this source offers, and which of them it uses.
///
/// A switch a row: what
/// the
/// row asks is whether this one is offered at all, and a shape that says on or
/// off needs no word beside it to be read. The three of them stand in a grid so
/// the switches, the names and the mark of the one in use each line up in a
/// column of their own: a row of names of different lengths would put that mark
/// somewhere new on every line.
///
/// A profile that asks this source for a value it has not got carries a warning
/// left of that mark rather than being switched off: whether it is offered is
/// the user's answer and not this page's, and a control that turns itself over
/// is a control nobody set. What it wants is on the warning, and the names
/// above it are each a button that makes its row.
///
/// Nothing ticked is every script there is, which is what a source that was
/// never asked means; ticking them all is written down the same way, so a
/// script installed later is offered rather than quietly left out.
fn offered_scripts(ui: &mut egui::Ui, app: &mut App, key: &SourceKey) {
    ui.label(egui::RichText::new(t!("settings.transfer_scripts")).strong());
    ui.label(egui::RichText::new(t!("settings.scripts_of_source_hint")).weak());

    let names: Vec<String> = app
        .scripts
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    let wants: std::collections::BTreeMap<String, Vec<String>> = app
        .scripts
        .entries()
        .iter()
        .map(|entry| {
            (
                entry.id.clone(),
                unanswered(app, key, &entry.manifest.variables),
            )
        })
        .collect();
    let origins: std::collections::BTreeMap<String, String> = app
        .scripts
        .entries()
        .iter()
        .map(|entry| (entry.id.clone(), origin_of(entry)))
        .collect();
    let chosen = app
        .memory(key)
        .map(|memory| memory.scripts.clone())
        .unwrap_or_default();
    let current = app
        .memory(key)
        .and_then(|memory| memory.script.clone())
        .unwrap_or_default();

    let mut offered: Vec<String> = Vec::new();
    let mut use_now = None;
    let mut changed = false;

    egui::Grid::new(ui.make_persistent_id(("scripts", key)))
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for name in &names {
                let mut shown = chosen.is_empty() || chosen.contains(name);
                changed |= crate::ui::widgets::switch(ui, &mut shown)
                    .on_hover_text(t!("settings.scripts_of_source_hint"))
                    .changed();
                let said = origins.get(name).cloned().unwrap_or_default();
                ui.label(name).on_hover_text(said);

                let missing = wants.get(name).cloned().unwrap_or_default();
                if missing.is_empty() {
                    ui.label("");
                } else {
                    ui.label(
                        egui::RichText::new(icons::WARNING).color(ui.visuals().error_fg_color),
                    )
                    .on_hover_text(t!("settings.script_wants", names = missing.join(", ")));
                }

                if shown {
                    if ui
                        .selectable_label(&current == name, icons::CURRENT)
                        .on_hover_text(t!("settings.script_use"))
                        .clicked()
                    {
                        use_now = Some(name.clone());
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();

                if shown {
                    offered.push(name.clone());
                }
            }
        });

    if changed {
        if let Some(memory) = app.memory_mut(key) {
            memory.scripts = if offered.len() == names.len() {
                Vec::new()
            } else {
                offered
            };
        }
        app.save_memory(key);
    }
    if let Some(name) = use_now {
        if let Some(memory) = app.memory_mut(key) {
            memory.script = Some(name);
        }
        app.save_memory(key);
    }
}

/// One script of the page: what it is called, what it offers and where it
/// stands.
struct Found {
    /// The name a person reads.
    name: String,
    /// The name of its directory, which is what everything else goes by.
    id: String,
    /// Which ways it carries a file.
    directions: String,
    /// Its own directory, which is where its manifest is.
    directory: std::path::PathBuf,
    /// Which directory of scripts it came from and what it stands in for.
    said: String,
}

/// Where a script was found, and what it stands in for.
fn origin_of(entry: &zyt_script::Entry) -> String {
    let mut said = t!(
        "settings.script_origin",
        directory = entry.root.to_string_lossy()
    )
    .to_string();
    for shadowed in &entry.shadowed {
        said.push('\n');
        said.push_str(&t!(
            "settings.script_shadowed",
            path = shadowed.to_string_lossy()
        ));
    }
    said
}

/// What a program may do through the operating system commands: one row per
/// sequence, one column per kind of session.
///
/// A program of this machine and a device on the other end of a line are not
/// the same guest, so each sequence is answered twice.
fn osc(ui: &mut egui::Ui, settings: &mut crate::config::Settings, app: &mut App) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.osc"));

    egui::Grid::new(ui.make_persistent_id("osc"))
        .num_columns(3)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("");
            trust_heading(ui, true);
            trust_heading(ui, false);
            ui.end_row();

            osc_name(
                ui,
                "settings.clipboard",
                "OSC-52",
                Some("settings.clipboard_hint"),
            );
            clipboard_choice(ui, app, false);
            clipboard_choice(ui, app, true);
            ui.end_row();

            for sequence in crate::config::OscSequence::ALL.iter().copied() {
                osc_name(
                    ui,
                    sequence.label_key(),
                    sequence.code(),
                    sequence.hint_key(),
                );
                changed |= allowed_choice(ui, settings.osc.trusted.switch(sequence));
                changed |= allowed_choice(ui, settings.osc.untrusted.switch(sequence));
                ui.end_row();
            }
        });

    changed
}

/// The name of the column of these settings that answers a session of that
/// kind.
///
/// It is the heading of the plate the sign of trust raises
/// (`statusbar::trust_plate`), where it stands over the sequences that session
/// is allowed. It lives here rather than beside the status bar because the name
/// is this page's own: a plate that called a column one thing while the page
/// called it another would be two names for one decision.
pub fn trust_hint(trusted: bool) -> String {
    let key = if trusted {
        "settings.osc_trusted"
    } else {
        "settings.osc_untrusted"
    };
    t!(key).to_string()
}

/// The heading of one column: the sign of that kind of session and its name.
///
/// The sign carries the color it carries everywhere else and no hint of its
/// own: a heading that has to be hovered to be read is a heading that was not
/// written, so the name stands beside it.
fn trust_heading(ui: &mut egui::Ui, trusted: bool) {
    let (icon, key) = if trusted {
        (icons::TRUSTED, "settings.osc_trusted")
    } else {
        (icons::UNTRUSTED, "settings.osc_untrusted")
    };

    ui.horizontal(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(egui::RichText::new(icon).color(icons::trust_color(ui, trusted)));
        ui.label(egui::RichText::new(t!(key)).strong());
    });
}

/// The cell naming what a program may do with the clipboard.
///
/// The cell shows the setting in use; the three of them are the menu it opens.
fn clipboard_choice(ui: &mut egui::Ui, app: &mut App, untrusted: bool) {
    let setting = match untrusted {
        true => app.settings.osc.untrusted.clipboard,
        false => app.settings.osc.trusted.clipboard,
    };
    let shown = t!(setting.label_key()).to_string();

    crate::ui::choice::row(
        ui,
        app,
        crate::ui::choice::Choice::Clipboard { untrusted },
        &shown,
    );
}

/// The cell choosing whether a sequence is honoured at all.
///
/// Two values are a switch, not a list to open: the shape says which way it
/// stands, and the hint says it in words.
fn allowed_choice(ui: &mut egui::Ui, allowed: &mut bool) -> bool {
    let hint = if *allowed {
        t!("settings.osc_allowed")
    } else {
        t!("settings.osc_denied")
    };

    crate::ui::widgets::switch(ui, allowed)
        .on_hover_text(hint)
        .changed()
}

/// The two menus of a path, side by side, each lined up in a grid of its own:
/// the switch that shows an entry, its name, and what else the entry needs.
/// The menus of a path, folded away under the sequence that makes one.
///
/// They are the menus of a `file://` address a console printed, which is OSC 8
/// and nothing else, and they are long: two columns of switches nobody reads
/// twice. Folded, the section says what it is about in one line.
/// How much of the command history is kept, folded away under the sequence
/// that fills it.
///
/// It sits beside the menus of a path rather than with the scrollback, because
/// what it belongs to is OSC 133: a console whose shell says nothing has no
/// history whatever this number says.
fn osc133_history(ui: &mut egui::Ui, settings: &mut crate::config::Settings) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(egui::RichText::new(t!("settings.osc133_history")).heading())
        .id_salt("osc133_history")
        .show(ui, |ui| {
            egui::Grid::new(ui.make_persistent_id("osc133_history"))
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t!("settings.command_history"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut settings.command_history).range(0..=10_000))
                        .on_hover_text(t!("settings.command_history_hint"))
                        .changed();
                    ui.end_row();

                    let keys = &mut settings.command_history_keys;
                    ui.label(t!("settings.command_history_enter"))
                        .on_hover_text(t!("settings.command_history_keys_hint"));
                    changed |= history_action(ui, &mut keys.enter);
                    ui.end_row();

                    ui.label(t!("settings.command_history_shift_enter"))
                        .on_hover_text(t!("settings.command_history_keys_hint"));
                    changed |= history_action(ui, &mut keys.shift_enter);
                    ui.end_row();
                });
            if settings.command_history == 0 {
                ui.label(
                    egui::RichText::new(t!("settings.command_history_empty"))
                        .weak()
                        .italics(),
                );
            }
        });
    changed
}

/// What one of the two keys does with the command under the selection, as the
/// two things there are to do with it side by side.
///
/// A pair of labels and not a switch, because neither of them is the other one
/// turned off: one types the command in and the other types it in and runs it,
/// and a switch would have to be read twice to say which way round it is.
fn history_action(ui: &mut egui::Ui, action: &mut crate::config::HistoryAction) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for offered in crate::config::HISTORY_ACTIONS.iter().copied() {
            if ui
                .selectable_label(*action == offered, t!(offered.label_key()))
                .on_hover_text(t!(offered.hint_key()))
                .clicked()
            {
                *action = offered;
                changed = true;
            }
        }
    });
    changed
}

/// What the menus a pointer opens over the terminal offer.
///
/// The menu of the terminal and the menu a `file://` address opens are one
/// subject: both are what the right button offers where it was pressed, and both
/// are read here by somebody deciding what that button should do. The entries of
/// an address are a page of their own inside it, because there are two menus of
/// them and a row each.
fn context_menu(ui: &mut egui::Ui, settings: &mut crate::config::Settings) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(egui::RichText::new(t!("settings.context_menu")).heading())
        .id_salt("context_menu")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                changed |= crate::ui::widgets::switch(ui, &mut settings.copy_saved_path).changed();
                ui.label(t!("settings.copy_saved_path"));
            });
            ui.add_space(8.0);
            changed |= osc8_menu(ui, settings);
        });
    changed
}

fn osc8_menu(ui: &mut egui::Ui, settings: &mut crate::config::Settings) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(egui::RichText::new(t!("settings.osc8_menu")).heading())
        .id_salt("osc8_menu")
        .show(ui, |ui| changed |= file_menu(ui, settings));
    changed
}

fn file_menu(ui: &mut egui::Ui, settings: &mut crate::config::Settings) -> bool {
    let mut changed = false;

    egui::Grid::new(ui.make_persistent_id("path_menus"))
        .num_columns(2)
        .spacing([32.0, 6.0])
        .show(ui, |ui| {
            ui.heading(t!("settings.file_menu"));
            ui.heading(t!("settings.directory_menu"));
            ui.end_row();

            ui.vertical(|ui| changed |= file_menu_rows(ui, &mut settings.file_menu));
            ui.vertical(|ui| changed |= directory_menu_rows(ui, &mut settings.directory_menu));
            ui.end_row();
        });

    changed
}

/// Rows of the menu of a file.
fn file_menu_rows(ui: &mut egui::Ui, menu: &mut crate::config::FileMenu) -> bool {
    let mut changed = false;

    egui::Grid::new(ui.make_persistent_id("file_menu_rows"))
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for (label, shown) in [
                ("link.open", &mut menu.open),
                ("link.open_with", &mut menu.open_with),
                ("file.rename", &mut menu.rename),
                ("file.move_here", &mut menu.move_here),
                ("link.copy_link", &mut menu.copy_link),
                ("link.copy_text", &mut menu.copy_text),
            ] {
                changed |= entry(ui, label, shown);
                ui.label("");
                ui.end_row();
            }

            changed |= entry(ui, "file.copy_contents", &mut menu.copy_contents);
            changed |= limit(ui, &mut menu.copy_limit);
            ui.end_row();

            changed |= entry(ui, "file.trash", &mut menu.trash);
            changed |= confirm(ui, &mut menu.trash_confirm);
            ui.end_row();

            changed |= entry(ui, "file.delete", &mut menu.delete);
            changed |= confirm(ui, &mut menu.delete_confirm);
            ui.end_row();
        });

    changed
}

/// Rows of the menu of a directory.
fn directory_menu_rows(ui: &mut egui::Ui, menu: &mut crate::config::DirectoryMenu) -> bool {
    let mut changed = false;

    egui::Grid::new(ui.make_persistent_id("directory_menu_rows"))
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for (label, shown) in [
                ("directory.open", &mut menu.open),
                ("link.open_with", &mut menu.open_with),
                ("file.rename", &mut menu.rename),
                ("file.move_here", &mut menu.move_here),
                ("link.copy_link", &mut menu.copy_link),
                ("link.copy_text", &mut menu.copy_text),
            ] {
                changed |= entry(ui, label, shown);
                ui.label("");
                ui.end_row();
            }

            changed |= entry(ui, "directory.trash", &mut menu.trash);
            changed |= confirm(ui, &mut menu.trash_confirm);
            ui.end_row();

            changed |= entry(ui, "directory.delete", &mut menu.delete);
            changed |= confirm(ui, &mut menu.delete_confirm);
            ui.end_row();
        });

    changed
}

/// The cell of one entry: a button that stays pressed while the entry is in the
/// menu.
fn entry(ui: &mut egui::Ui, label: &str, shown: &mut bool) -> bool {
    let response = ui
        .selectable_label(*shown, t!(label))
        .on_hover_text(t!("settings.menu_shown"));
    if response.clicked() {
        *shown = !*shown;
        return true;
    }
    false
}

/// The cell of a row that asks before it acts.
fn confirm(ui: &mut egui::Ui, confirm: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed = crate::ui::widgets::switch(ui, confirm)
            .on_hover_text(t!("settings.file_confirm_hint"))
            .changed();
        ui.label(egui::RichText::new(t!("settings.file_confirm")).weak());
    });
    changed
}

/// The cell holding the largest file whose contents are offered to the
/// clipboard.
fn limit(ui: &mut egui::Ui, limit: &mut u64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t!("settings.file_limit")).weak());
        let mut kib = *limit / 1024;
        if ui
            .add(
                egui::DragValue::new(&mut kib)
                    .range(1..=1024 * 1024)
                    .suffix(t!(crate::format::SIZE_UNITS[1])),
            )
            .on_hover_text(t!("settings.file_limit_hint"))
            .changed()
        {
            *limit = kib * 1024;
            changed = true;
        }
    });
    changed
}

/// Name of one sequence: what it does, and under it, weak, the number it has.
///
/// A sequence whose name says the whole of it carries no sentence, and the
/// pointer uncovers nothing over it.
fn osc_name(ui: &mut egui::Ui, label: &str, code: &str, hint: Option<&str>) {
    let named = ui
        .vertical(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.label(t!(label));
            ui.label(egui::RichText::new(code).weak().small());
        })
        .response;
    if let Some(hint) = hint {
        named.on_hover_text(t!(hint));
    }
}

/// The scripts that were found, where each came from and what it shadows.
///
/// A script is a file and not a setting, so there is nothing here to edit: the
/// page says what was found, which directory it came from, what it offers and
/// what could not be read as a script at all. The buttons are what somebody
/// writing one wants — read the directories again, open the directory a script
/// of their own belongs in — and the third is the one thing about the dialogs
/// of a script this page can say: every form that was told not to ask again is
/// put back to asking.
fn scripts(ui: &mut egui::Ui, app: &mut App) {
    ui.heading(t!("settings.scripts"));
    ui.add_space(8.0);

    let found: Vec<Found> = app
        .scripts
        .entries()
        .iter()
        .map(|entry| {
            let directions: Vec<String> =
                [zyt_script::Direction::Send, zyt_script::Direction::Receive]
                    .into_iter()
                    .filter(|way| entry.manifest.offers(*way))
                    .map(|way| match way {
                        zyt_script::Direction::Send => t!("settings.script_send").to_string(),
                        zyt_script::Direction::Receive => t!("settings.script_receive").to_string(),
                    })
                    .collect();
            Found {
                name: entry.manifest.name.clone(),
                id: entry.id.clone(),
                directions: directions.join(", "),
                directory: entry.directory.clone(),
                said: origin_of(entry),
            }
        })
        .collect();

    let open_hint = t!("settings.script_open_directory").to_string();
    let mut open = None;
    egui::Grid::new(ui.make_persistent_id("scripts-found"))
        .num_columns(4)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for script in &found {
                ui.label(egui::RichText::new(&script.name).strong())
                    .on_hover_text(&script.said);
                ui.label(egui::RichText::new(&script.id).weak());
                ui.label(&script.directions);
                let hint = format!("{open_hint}\n{}", script.directory.display());
                if ui.button(icons::FOLDER).on_hover_text(hint).clicked() {
                    open = Some(script.directory.clone());
                }
                ui.end_row();
            }
        });
    if let Some(directory) = open {
        app.open_directory(&directory);
    }

    let problems: Vec<(String, String)> = app
        .scripts
        .problems()
        .iter()
        .map(|problem| {
            (
                problem
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                problem.said.clone(),
            )
        })
        .collect();
    if !problems.is_empty() {
        ui.add_space(8.0);
        for (name, said) in &problems {
            ui.label(
                egui::RichText::new(format!("{} {name}", icons::WARNING))
                    .color(ui.visuals().error_fg_color),
            )
            .on_hover_text(said);
        }
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(t!("settings.script_reload")).clicked() {
            app.reload_scripts();
        }
        let mine = app.store.config_dir().join(zyt_script::SCRIPTS);
        if ui
            .button(format!("{} {}", icons::FOLDER, t!("settings.scripts_mine")))
            .on_hover_text(mine.to_string_lossy())
            .clicked()
        {
            app.open_scripts_directory();
        }
        if ui
            .button(t!("settings.forms_reset"))
            .on_hover_text(t!("settings.forms_reset_hint"))
            .clicked()
        {
            app.reset_forms();
        }
    });
}
