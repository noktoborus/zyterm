//! A command in a pseudo terminal of its own, behind a line of a given speed.
//!
//! It stands between this terminal and a program: run in the console of the
//! terminal, it starts the program in a pseudo terminal and carries the bytes
//! both ways at the speed of a serial line — 9600, 57600, 115200 baud or any
//! other — so how the terminal and the program behave on a slow line is seen
//! on a desk with no device on it. `tmux -CC`, `vim`, `mc`, `htop` work through
//! it as through a line: the outer terminal is in raw mode, the size of the
//! window follows it, and the alternate screen, the mouse and the keys of the
//! application mode pass as bytes like any other.

#[cfg(unix)]
mod frame;
#[cfg(unix)]
mod line;
#[cfg(unix)]
mod relay;
#[cfg(unix)]
mod signals;
#[cfg(unix)]
mod tty;

#[cfg(unix)]
fn main() {
    use clap::Parser as _;
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(tty::LEAD) {
        eprintln!("pty-throttle: {}", tty::lead(&args[2..]));
        std::process::exit(127);
    }
    let options = relay::Options::parse();
    match relay::run(options) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("pty-throttle: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!(
        "pty-throttle: a pseudo terminal of unix is what it throttles; there is no build for this platform"
    );
    std::process::exit(1);
}
