//! The window while a script runs: what stands for it, and what is gone with
//! it.

use zyt_script::Targets;

use super::window;

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
