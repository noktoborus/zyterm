//! What tmux sends back, read into the state: the output of the panes, the
//! replies matched to the commands sent, the notifications.

use std::time::Duration;

use rust_i18n::t;
use zyt_tmux::{Command, Notice, PaneId, Part, Reply, Sink};

use super::pane::{Output, Paused};
use super::{LAG_SHOWN, Phase, State, Todo, info_of};

impl Sink for State {
    fn output(&mut self, pane: PaneId, age: Option<u32>, bytes: &[u8]) {
        let shown = self.shown == Some(pane);
        let Some(entry) = self.pane_for_output(pane) else {
            return;
        };
        entry.age = age.map(|age| Duration::from_millis(u64::from(age)));
        if shown && entry.age.is_none_or(|age| age < LAG_SHOWN) {
            self.lost = false;
        }
        let Some(entry) = self.panes.get_mut(&pane) else {
            return;
        };
        if entry.output(bytes) == Output::Nested {
            self.told.push(t!("tmux.nested").to_string());
            self.todo.push(Todo::DetachNested(pane));
        }
    }

    fn reply_line(&mut self, command: Command, line: &[u8]) -> Reply {
        let go = match command {
            Command::Opening => {
                self.opening
                    .push(String::from_utf8_lossy(line).into_owned());
                true
            }
            Command::Version => {
                self.take_version(line);
                true
            }
            Command::ListSessions => {
                self.model.list_session(line);
                true
            }
            Command::ListWindows => {
                self.model.list_window(line);
                true
            }
            Command::ListPanes => {
                self.model.list_pane(line);
                true
            }
            Command::WindowInfo => {
                self.info = info_of(line);
                true
            }
            Command::Display(pane) => self
                .panes
                .get_mut(&pane)
                .is_none_or(|entry| entry.take_modes(line)),
            Command::Capture { pane, .. } => self
                .panes
                .get_mut(&pane)
                .is_none_or(|entry| entry.screen_line(line)),
            _ => true,
        };
        match go {
            true => Reply::Go,
            false => Reply::Broken,
        }
    }

    fn reply_end(&mut self, command: Command, failed: bool) {
        match command {
            Command::Opening if failed => {
                let reason = std::mem::take(&mut self.opening).join("; ");
                self.told
                    .push(t!("tmux.failed", reason = reason.as_str()).to_string());
                self.phase = Phase::Leaving;
            }
            Command::Opening => self.todo.push(Todo::AskVersion),
            Command::ListSessions => self.model.sessions_listed(),
            Command::ListWindows => {
                for pane in self.model.windows_listed() {
                    self.close(pane);
                }
                let model = &self.model;
                self.marks.retain(|window| model.window(window).is_some());
            }
            Command::ListPanes => {
                self.model.panes_listed();
                self.lists_changed = true;
                self.todo.push(Todo::Show);
            }
            Command::Display(pane)
                if self.panes.get(&pane).is_some_and(|entry| {
                    entry.deeper.is_some() && entry.state != super::pane::PaneState::Loading
                }) =>
            {
                match failed {
                    true => {
                        if let Some(entry) = self.panes.get_mut(&pane) {
                            entry.deeper = None;
                        }
                    }
                    false => self.todo.push(Todo::Deep(pane)),
                }
            }
            Command::Display(pane)
            | Command::Capture {
                pane,
                part: Part::Saved | Part::History { .. },
            } if failed => {
                if let Some(entry) = self.panes.get_mut(&pane) {
                    entry.snapshot_failed = true;
                }
            }
            Command::Capture {
                pane,
                part: Part::Saved,
            } => {
                if let Some(entry) = self.panes.get_mut(&pane) {
                    entry.saved_screen_end();
                }
            }
            Command::Capture {
                pane,
                part: Part::Visible | Part::Range { .. },
            } => self.snapshot_end(pane, failed),
            Command::SendKeys(_) => self.todo.push(Todo::Keys),
            Command::WindowInfo => self.lists_changed = true,
            _ => {}
        }
    }

    fn notice(&mut self, notice: Notice<'_>) {
        match notice {
            Notice::Layout {
                window,
                layout,
                visible,
                ..
            } => self.layout(window, layout, visible),
            Notice::WindowAdded(window) => self.model.add(window),
            Notice::WindowClosed(window) | Notice::UnlinkedWindowClosed(window) => {
                self.marks.clear(window);
                for pane in self.model.close(window) {
                    self.close(pane);
                }
            }
            Notice::WindowRenamed(window, name) | Notice::UnlinkedWindowRenamed(window, name) => {
                self.model.rename(window, name)
            }
            Notice::SessionWindowChanged(session, window) => {
                self.model.window_changed(session, window);
                if self.model.session == Some(session) && self.version.is_some() {
                    self.todo.push(Todo::Show);
                }
            }
            Notice::PaneChanged(window, pane) => {
                self.model.pane_changed(window, pane);
                if self.model.current_window() == Some(window) && self.version.is_some() {
                    self.todo.push(Todo::Show);
                }
            }
            Notice::SessionChanged(session, name) => {
                self.model.session_changed(session, name);
                if self.version.is_some() {
                    self.todo.push(Todo::Show);
                }
            }
            Notice::SessionRenamed(session, name) => self.model.name_session(session, name),
            Notice::Exit(reason) => {
                log::info!("tmux: %exit {}", reason.unwrap_or_default());
                self.phase = Phase::Leaving;
            }
            Notice::Unknown(word) => {
                if self.unknown.insert(word.to_vec()) {
                    log::debug!("tmux: {} is not read here", String::from_utf8_lossy(word));
                }
            }
            Notice::SessionsChanged | Notice::UnlinkedWindowAdded(_) => {
                if self.version.is_some() && !self.todo.contains(&Todo::Lists) {
                    self.todo.push(Todo::Lists);
                }
            }
            Notice::SubscriptionChanged {
                name,
                window,
                value,
            } => {
                if name == zyt_tmux::BELLS {
                    self.model.ring(window, value.trim() == "1");
                    self.lists_changed = true;
                }
            }
            Notice::Paused(pane) => {
                if let Some(entry) = self.panes.get_mut(&pane) {
                    entry.paused = Some(Paused {
                        at: std::time::Instant::now(),
                        behind: entry.age.unwrap_or_default(),
                    });
                    if self.shown == Some(pane) {
                        self.lost = true;
                        self.todo.push(Todo::Resume(pane));
                    }
                }
            }
            Notice::Continued(_) => {}
        }
    }
}
