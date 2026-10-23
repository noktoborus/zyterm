//! The run of a script on the line of a session: its start, what it says,
//! how it is stopped and its end once the line is free.
//!
//! ```text
//! Running ──Cancel──► Asked ──zyt.run.ending()──► Ending ──it ended──► Draining ──line free──► end
//!    │                  │                            │                    │
//!    │                  └────────── Force, Ctrl+Esc ─┴────────────────────┴──► end
//!    └──zyt.run.ending(), by itself──► Ending
//! ```
//!
//! No clock moves a run from one phase to the next: the script does, or the
//! person does. A disconnect and the window closing are a Force.

use super::{ForcedBy, RunPhase, ScriptLife, Session, Source, Told};
use crate::error::{AppError, Result};
use crate::screen::ScreenCommand;
use rust_i18n::t;
use std::time::{Duration, Instant};
use zyt_script::{Direction, Line as _, Outcome, ScriptEvent, ScriptRun, Targets};

impl Session {
    /// Starts a script on the line of this session.
    ///
    /// One script runs at a time and it owns the line while it does: nothing
    /// of the device reaches the terminal, the keyboard writes nothing, and
    /// the answers the terminal owes a program wait. A script that says it
    /// does not hold the line is given one that is already shut, so what it
    /// writes there is refused rather than quietly dropped.
    pub fn start_script(
        &mut self,
        entry: &zyt_script::Entry,
        roots: &[std::path::PathBuf],
        direction: Direction,
        target: Targets,
        variables: &std::collections::BTreeMap<String, String>,
        wake: std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Result<()> {
        if matches!(self.source, Source::None) {
            return Err(AppError::NotConnected);
        }
        if self.is_transferring() {
            return Err(AppError::TransferRunning);
        }
        if self.view.said.tmux.look.is_some() {
            return Err(AppError::TmuxHoldsLine);
        }
        // What a program of the device said about itself is about a program
        // the line no longer reaches once a script takes it.
        if entry.manifest.hold_line {
            self.terminal_progress = None;
        }

        let holds_line = entry.manifest.hold_line;
        let port = match &self.source {
            Source::Serial { supervisor, .. } => Some(crate::port::ScriptPort::new(
                supervisor.clone(),
                self.port_changed.clone(),
            )),
            _ => None,
        };
        let channel = self.link.run(holds_line, self.resizer(), port);

        let cancel = zyt_script::Cancel::new();
        let shown = zyt_script::ByteSwap::with_capacity(0);
        self.send(ScreenCommand::Echo(Some(shown.clone())));
        let (talker, heard) = crate::scripts::talker(
            wake,
            shown,
            self.screen.waker(),
            self.screen.state_reader(),
            self.off.clone(),
            cancel.clone(),
        );
        let start = zyt_script::Start::of_entry(
            entry,
            roots,
            direction,
            target,
            variables.clone(),
            zyt_script::Talking {
                line: channel.clone(),
                prompt: talker,
                cancel,
            },
        );

        let life = ScriptLife {
            id: entry.id.clone(),
            name: entry.manifest.name.clone(),
            holds_line,
            started: Instant::now(),
            ended: None,
            progress: None,
            stage: None,
            bytes: None,
            pace: crate::rate::Pace::default(),
            speed: None,
            direction,
            asked: false,
            ending: false,
        };

        // The start is told by nothing but the plate of the run, which stands
        // from this frame on.
        let run = ScriptRun::start(start, direction)?;

        self.script = Some(run);
        self.line = Some(channel);
        self.heard = Some(heard);
        self.run = Some(life);
        self.runs += 1;
        Ok(())
    }

    /// What the running script is called, while one runs.
    pub fn script_name(&self) -> Option<&str> {
        self.run.as_ref().map(|life| life.name.as_str())
    }

    /// The directory of the running script, which is what it is known by.
    pub fn script_id(&self) -> Option<&str> {
        self.run.as_ref().map(|life| life.id.as_str())
    }

    /// The dialog a script is waiting on, handed over once.
    pub fn take_ask(&mut self) -> Option<crate::scripts::Waiting> {
        self.waiting.take()
    }

    /// Notices a script that is over.
    pub(super) fn tick_script(&mut self) {
        let Some(script) = self.script.as_mut() else {
            return;
        };
        if !script.tick() {
            return;
        }

        let outcome = script.outcome();
        let failure = script.take_failure();
        self.script = None;
        self.report_script_end(outcome, failure);
    }

    /// Takes in everything the script said since the last frame.
    pub(super) fn drain_script(&mut self) {
        let mut said: Vec<ScriptEvent> = Vec::new();
        let mut asked = None;
        if let Some(heard) = &self.heard {
            while let Ok(event) = heard.events.try_recv() {
                said.push(event);
            }
            while let Ok(pending) = heard.asks.try_recv() {
                asked = Some(pending);
            }
        }

        for event in said {
            match event {
                ScriptEvent::Progress(progress) => {
                    if let Some(life) = self.run.as_mut() {
                        life.progress = crate::scripts::progress_of(progress);
                        // Putting the progress away puts away all of what
                        // the script said about where it stands.
                        if life.progress.is_none() {
                            life.stage = None;
                            life.bytes = None;
                            life.pace = crate::rate::Pace::default();
                            life.speed = None;
                        }
                    }
                }
                ScriptEvent::Stage(stage) => {
                    if let Some(life) = self.run.as_mut() {
                        life.stage = Some(stage);
                    }
                }
                ScriptEvent::Bytes { done, whole } => {
                    if let Some(life) = self.run.as_mut() {
                        life.bytes = Some((done, whole));
                        life.pace.push(done);
                    }
                }
                ScriptEvent::Speed(speed) => {
                    if let Some(life) = self.run.as_mut() {
                        life.speed = Some(speed);
                    }
                }
                ScriptEvent::Ending => {
                    if let Some(life) = self.run.as_mut() {
                        life.ending = true;
                    }
                }
                ScriptEvent::Finished { .. } => {}
            }
        }

        if let Some(pending) = asked
            && let Some(dropped) = self.waiting.replace(pending.of_run(self.runs))
        {
            dropped.dismiss();
        }
    }

    /// Forces the end of the run: the source is being let go of, and nobody
    /// stands at the plate to choose.
    pub fn stop_transfers(&mut self) {
        self.force_transfer(ForcedBy::Disconnect);
    }

    /// Ends the run at once, whatever the script is doing, and says so in the
    /// log.
    ///
    /// The line is the screen's again; what the script wrote and the screen
    /// had not carried yet goes out first, and what was handed to the source
    /// stays on its way. The programs of the script are killed, its sockets
    /// closed, and its thread is left as it is.
    pub fn force_transfer(&mut self, by: ForcedBy) {
        if let Some(waiting) = self.waiting.take() {
            waiting.dismiss();
        }
        let (Some(life), Some(phase)) = (self.run.as_ref(), self.run_phase()) else {
            return;
        };
        log::warn!(
            "the run {} was forced to end by {by}, in phase {phase:?}",
            life.name
        );
        if let Some(mut script) = self.script.take() {
            script.force();
            self.report_script_end(Some(Outcome::Cancelled), None);
        }
        self.end_run();
    }

    /// Takes in what the run changed on the port, so the window shows it.
    /// The parameters are written down by the window when the run ends.
    pub(super) fn take_port_changes(&mut self) {
        let changed = std::mem::take(
            &mut *self
                .port_changed
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        if let Some(params) = changed.params {
            self.params = params;
            self.params_of_run = true;
        }
        self.rts_hold = changed.rts.unwrap_or(self.rts_hold);
        self.dtr_hold = changed.dtr.unwrap_or(self.dtr_hold);
        self.held_break = changed.held_break.unwrap_or(self.held_break);
        self.read_hold = changed.read_hold.unwrap_or(self.read_hold);
    }

    /// True once after a run that changed the parameters of the line ended:
    /// the window writes them down then.
    pub fn take_params_of_run(&mut self) -> bool {
        if self.run.is_some() {
            return false;
        }
        std::mem::take(&mut self.params_of_run)
    }

    /// What gives a console and the grid the size of a far terminal, for
    /// the run. The grid is held to the area and `screen_overflow` past it, at
    /// least 2 by 1; a serial port takes no size.
    fn resizer(&self) -> crate::line::Resize {
        let Source::Console { session, .. } = &self.source else {
            return std::sync::Arc::new(|_, _| false);
        };
        let (console, send, cell) = (session.clone(), self.screen.sender(), self.cell_told);
        let limit = (self.area.0 + self.overflow.0, self.area.1 + self.overflow.1);
        std::sync::Arc::new(move |columns, rows| {
            if console.resize(columns, rows, cell).is_err() {
                return false;
            }
            send(ScreenCommand::Resize {
                columns: usize::from(columns).min(limit.0).max(2),
                rows: usize::from(rows).min(limit.1).max(1),
            });
            true
        })
    }

    /// Puts away everything a run owns, however it ended: the script and what
    /// it started, the line it was given, what it said, the dialog it waits
    /// on, and how far it said it was.
    ///
    /// Every way a run ends comes here, so none of them leaves a part of the
    /// run behind for the next one to find: a share standing in the status
    /// bar after a disconnect, a dialog still asking for a script that is
    /// gone.
    pub(super) fn end_run(&mut self) {
        if let Some(mut script) = self.script.take() {
            script.abandon();
        }
        if let Some(line) = self.line.take() {
            line.close();
            self.send(ScreenCommand::Echo(None));
        }
        if self.run.take().is_some() {
            self.run_ended = true;
        }
        *self.off.lock().unwrap_or_else(|error| error.into_inner()) = Default::default();
        let (columns, rows) = self.area;
        self.send(ScreenCommand::Resize { columns, rows });
        self.tell_console();
        self.heard = None;
        if let Some(waiting) = self.waiting.take() {
            waiting.dismiss();
        }
    }

    /// The number of the run standing, from its start to the end of its line.
    ///
    /// A dialog carries the number of the run that asked it, and one whose
    /// number is not this one belongs to a run that is gone.
    pub fn run_number(&self) -> Option<u64> {
        self.run.as_ref().map(|_| self.runs)
    }

    /// How long the running transfer has been going on.
    pub fn transfer_elapsed(&self) -> Option<Duration> {
        self.run.as_ref().map(|run| run.started.elapsed())
    }

    /// How long the last finished transfer took.
    pub fn last_transfer(&self) -> Option<Duration> {
        self.last_transfer
    }

    /// Reports how the script ended and starts waiting for the line.
    pub(super) fn report_script_end(
        &mut self,
        outcome: Option<Outcome>,
        failure: Option<zyt_script::ScriptError>,
    ) {
        let Some(life) = self.run.as_mut() else {
            return;
        };
        if life.ended.is_some() {
            return;
        }
        life.ended = Some(Instant::now());

        let name = life.name.clone();
        let took = crate::format::duration(life.started.elapsed());
        log::info!("the script {name} ended: {outcome:?}");

        // A run that did what it was started for is told by its plate going
        // away; one that did not is told why, which nothing on the screen says.
        let said = failure.map(|error| crate::error::said(&error));
        let text = match outcome {
            Some(Outcome::Done) => return,
            Some(Outcome::Cancelled) => t!("script.cancelled", script = name, took = took),
            _ => t!("script.failed", script = name, took = took),
        };
        self.tell(text.to_string(), said.into_iter().collect());
    }

    /// Ends the run once the last byte left the port.
    ///
    /// The script being over is not the run being over: on a slow line the
    /// driver still holds bytes, and the run stands until the device has had
    /// all of them.
    pub(super) fn end_when_line_is_free(&mut self) {
        let Some(life) = self.run.as_ref() else {
            return;
        };
        if life.ended.is_none() {
            return;
        }
        if self.script.is_some() || self.line_busy() {
            return;
        }

        self.last_transfer = Some(life.started.elapsed());
        self.end_run();
    }

    /// How long the line was busy the last time, when that was worth naming.
    pub fn last_busy(&self) -> Option<Duration> {
        self.last_busy
    }

    /// Tracks how long the line stays busy, so the interface can name the last
    /// period after it ended.
    pub(super) fn track_line_busy(&mut self) {
        let busy = self.pending_output() > 0;
        match (busy, self.busy_since) {
            (true, None) => self.busy_since = Some(Instant::now()),
            (false, Some(since)) => {
                let span = since.elapsed();
                self.busy_since = None;
                if span >= Duration::from_secs(1) {
                    self.last_busy = Some(span);
                }
            }
            _ => {}
        }
    }

    /// Asks the running script to stop: the flag, and its `cleanup`.
    ///
    /// It answers at once. The script ends when it ends, and says so by
    /// `zyt.run.ending()` meanwhile; the plate offers Force from here on.
    pub fn cancel_transfer(&mut self) -> Result<()> {
        if let Some(waiting) = self.waiting.take() {
            waiting.dismiss();
        }
        let Some(script) = self.script.as_mut() else {
            return Ok(());
        };
        if script.is_finished() {
            return Ok(());
        }
        script.cancel();
        if let Some(life) = self.run.as_mut() {
            life.asked = true;
        }
        Ok(())
    }

    /// How far along the running script says it is, when it says: what the
    /// plate of the run and the panel of what runs show. It goes with the run.
    pub fn run_progress(&self) -> Option<zyt_term::ProgressState> {
        self.run.as_ref().and_then(|life| life.progress)
    }

    /// Where the running script says it stands, when it said.
    pub fn run_stage(&self) -> Option<&zyt_script::Stage> {
        self.run.as_ref().and_then(|life| life.stage.as_ref())
    }

    /// How many bytes the running script says went, and of how many.
    pub fn run_bytes(&self) -> Option<(u64, Option<u64>)> {
        self.run.as_ref().and_then(|life| life.bytes)
    }

    /// How fast the bytes the running script reports go, in kibibytes a
    /// second, once there were two reports to measure between — unless the
    /// script says its speed itself, which then stands alone.
    pub fn run_speed(&self) -> Option<f32> {
        self.run
            .as_ref()
            .filter(|life| life.speed.is_none())
            .and_then(|life| life.pace.per_second())
    }

    /// How fast the running script said it goes, each way, when it said so.
    pub fn run_said_speed(&self) -> Option<zyt_script::Speed> {
        self.run.as_ref().and_then(|life| life.speed)
    }

    /// Which way the running script carries files.
    pub fn run_direction(&self) -> Option<Direction> {
        self.run.as_ref().map(|life| life.direction)
    }

    /// True while the run standing has the line to itself, its last bytes
    /// included: what the plate of a run stands for.
    pub fn run_holds_line(&self) -> bool {
        self.run.as_ref().is_some_and(|life| life.holds_line)
    }

    /// Where the run standing is.
    pub fn run_phase(&self) -> Option<RunPhase> {
        let life = self.run.as_ref()?;
        Some(if life.ended.is_some() {
            RunPhase::Draining
        } else if life.ending {
            RunPhase::Ending
        } else if life.asked {
            RunPhase::Asked
        } else {
            RunPhase::Running
        })
    }

    /// What the desktop is told about progress: the script while one runs,
    /// the program of the device otherwise.
    pub fn outside_progress(&self) -> Option<zyt_term::ProgressState> {
        match &self.run {
            Some(_) => self.run_progress(),
            None => self.terminal_progress,
        }
    }

    /// True while a script runs, or waits for the line to drain.
    pub fn is_transferring(&self) -> bool {
        self.script.is_some() || self.run.is_some()
    }

    /// What a run came to since the last call, oldest first.
    ///
    /// The terminal is where the output of the device stands, so what this
    /// program has to say of a run goes to the desktop: the session keeps it
    /// until the window, which knows the desktop, asks.
    pub fn take_told(&mut self) -> Vec<Told> {
        std::mem::take(&mut self.told)
    }

    pub(super) fn tell(&mut self, message: String, details: Vec<String>) {
        self.told.push(Told { message, details });
    }
}
