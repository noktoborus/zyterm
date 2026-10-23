# Tools

This file lists which tool to use for each task. The rules for using them are
in `AGENTS.md`.

`/opt/packages.sh` installs the tools that the image of the container does
not have. A command that uses them runs after `. /opt/env.sh`.

## Reading code

Read a summary first. Open a file only when the summary does not answer.

| task | tool |
| --- | --- |
| find text | `rg` |
| find code by its syntax | `ast-grep run -l rust -p '<pattern>'` |
| change code by its syntax, in many places | `ast-grep run -l rust -p '<pattern>' -r '<new>' -U` |
| the public API of a crate, ours or a dependency | `cargo brief`, `ruskel` |
| the modules and items of a package | `tools/inventory.sh map <package>` |
| the source of a dependency | `cargo fetch`, then the registry of cargo |

## Measuring code

`tools/inventory.sh` prints a short report. The full output is in
`target/inventory`.

| question | command | tool under it |
| --- | --- | --- |
| how many lines, per crate | `size` | rustloc |
| how the lines changed since a commit | `size <rev>` | rustloc |
| the largest files | `files [n]` | rustloc |
| code that is copied | `clones [tokens]` | PMD CPD, Rust and Lua |
| what the tests run | `cover [n]` | cargo-llvm-cov |
| dependencies that no code uses | `deps` | cargo-shear |

A function that no test runs is one of two things:

- dead code: delete it;
- a feature with no test: write the test.

A clone is one of two things:

- one subject in two places: move it to one place;
- two subjects that look the same: leave it.

## Checking a change

| check | command |
| --- | --- |
| format | `cargo fmt --all` |
| lints | `cargo clippy --workspace --all-targets` |
| build | `cargo build --workspace` |
| tests | `cargo test --workspace` |
| advisories | `cargo audit` |
| unused dependencies | `tools/inventory.sh deps` |

## Testing

| what is tested | tool |
| --- | --- |
| the window: its tree, its keys, what goes to the line | `egui_kittest` |
| the picture of a widget | `egui_kittest` with `snapshot` |
| a slow serial line | `experiments/pty-throttle` |
| tmux control mode on old and new releases | `tmux-3.0a`, `tmux-3.2a`, `tmux-3.4`, `tmux-3.8` |
| the window on a real display | Xvfb with xdotool, or cage with wtype and grim |
| what a screen reader reads | `tools/atspi-tree` |

## Answering a question

Each experiment answers one question. None of them is a part of the program.

| question | tool |
| --- | --- |
| which glyphs the fonts of the toolkit have | `cargo run -p glyphs` |
| what the terminal sends for a key | `cargo run -p keys` |
| what a terminal does when the bytes come faster | `cargo run -p flood` |
| what a sixel payload holds and what it costs | `cargo run -p sixel` |
| what a real tmux says in control mode | `cargo run -p tmux-cc` |
