//! The icon the executable carries on Windows.
//!
//! An icon resource is what the shell draws the file with: in a folder, on the
//! taskbar, in the start menu and in the dialog that asks which program should
//! open something. Without one the shell has nothing to draw but its default
//! icon, and that is before the program has been started at all. The icon the
//! window carries is a separate thing, set at run time from `assets/icon.png`.
//!
//! The resource compiler is the one of the platform, so a toolchain without it
//! gets a warning and an executable with no icon rather than a failed build.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icon.ico");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the icon of the executable was not built in: {error}");
    }
}
