//! The window while a script runs: what stands for it, and what is gone with
//! it.

use egui_kittest::kittest::Queryable;
use rust_i18n::t;
use zyt_script::Targets;

use super::{Window, window};

/// Starts the test script `name` in the window.
fn started(window: &mut Window, name: &str) {
    crate::session::start_fixture(
        &mut window.harness.state_mut().session,
        name,
        Targets::none(),
    )
    .expect("the script starts");
}

/// A dialog standing when its run is stopped is closed: nothing is left to
/// answer it.
#[test]
fn the_dialog_of_a_run_that_was_stopped_is_closed() {
    let mut window = window();
    crate::session::start_fixture(
        &mut window.harness.state_mut().session,
        "asks",
        Targets::none(),
    )
    .expect("the script starts");
    window.step_until("the dialog of the script", |window| {
        window.shows("Asked by a script")
    });

    window
        .harness
        .state_mut()
        .session
        .cancel_transfer()
        .expect("the script stops");
    window.step_until("the dialog gone", |window| {
        !window.shows("Asked by a script") && window.harness.state().ui.form.is_none()
    });
}

/// A script holding the line stands on a plate in the middle of the terminal:
/// what it is, where it stands, the bytes, and a button that stops it.
#[test]
fn a_run_stands_on_a_plate_and_is_stopped_from_it() {
    let mut window = window();
    started(&mut window, "stands");
    let stage = format!(
        "{} · foo.bin · {}",
        t!("transfer.stage.sending"),
        t!("transfer.stage.of_files", file = 2, files = 5)
    );
    window.step_until("the stage on the plate", |window| window.shows(&stage));
    assert!(window.shows(&format!(
        "Says where it stands · {}",
        t!("transfer.sending")
    )));
    let of = t!(
        "transfer.bytes_of",
        done = crate::format::size(2097152),
        whole = crate::format::size(4194304)
    );
    assert!(window.shows(&format!("{of} · 50%")));

    // The toolkit measures an area on its first frame and places it on the
    // next.
    window.harness.run_steps(2);
    // The text starts at the left of the plate and the button ends at its
    // right, so the middle of the two is the middle of the plate.
    let left = window.harness.get_by_label(&stage).rect().left();
    let right = window
        .harness
        .get_by_label(&t!("transfer.cancel"))
        .rect()
        .right();
    assert!(
        ((left + right) / 2.0 - 500.0).abs() < 2.0,
        "in the middle of the terminal, which is as wide as the window: {left}..{right}"
    );

    window.harness.get_by_label(&t!("transfer.cancel")).click();
    window.step_until("the run gone", |window| {
        !window.harness.state().session.is_transferring()
    });
    window.harness.run_steps(2);
    assert!(!window.shows(&stage), "the plate went with it");
}

/// While a dialog of the script stands, the plate does not: the run waits on
/// the dialog, and two things in the middle of the window are one too many.
#[test]
fn the_plate_stands_aside_for_a_dialog_of_the_script() {
    let mut window = window();
    started(&mut window, "asks");
    window.step_until("the dialog of the script", |window| {
        window.shows("Asked by a script")
    });
    assert!(
        window
            .harness
            .query_by_label(&t!("transfer.cancel"))
            .is_none()
    );
    window
        .harness
        .state_mut()
        .session
        .cancel_transfer()
        .expect("the script stops");
}

/// A script beside the line has no plate: the terminal is what its window is
/// doing. Its share stands in its row of the panel of what runs.
#[test]
fn a_run_beside_the_line_shows_its_share_in_the_panel() {
    let mut window = window();
    started(&mut window, "beside");
    window.step_until("the share of the run", |window| {
        window.harness.state().session.run_progress().is_some()
    });
    window.harness.state_mut().show_tasks = true;
    window.harness.run_steps(3);

    assert!(window.shows("40%"), "the share, in the row of the run");
    assert!(
        window
            .harness
            .query_by_label(&t!("transfer.cancel"))
            .is_none(),
        "and no plate over the terminal"
    );
    window
        .harness
        .state_mut()
        .session
        .cancel_transfer()
        .expect("the script stops");
}

/// A run that says nothing of how far it is stands on a plate with no bar,
/// and one that says it has the bar.
#[test]
fn the_plate_has_a_bar_only_for_a_run_that_says_how_far_it_is() {
    let bars = |window: &Window| {
        window
            .harness
            .query_all_by_role(egui::accesskit::Role::ProgressIndicator)
            .count()
    };

    let mut window = window();
    started(&mut window, "holds");
    window.step_until("the plate of the run", |window| {
        window.shows(&t!("transfer.cancel"))
    });
    window.harness.run_steps(2);
    assert_eq!(bars(&window), 0, "no bar for a run that says nothing");
    window
        .harness
        .state_mut()
        .session
        .cancel_transfer()
        .expect("the script stops");

    let mut window = super::window();
    started(&mut window, "reports");
    window.step_until("the plate of the run", |window| {
        window.shows(&t!("transfer.cancel"))
    });
    window.harness.run_steps(2);
    assert_eq!(bars(&window), 1, "and one for a run that says it");
    window
        .harness
        .state_mut()
        .session
        .cancel_transfer()
        .expect("the script stops");
}
