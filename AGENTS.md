# AGENTS

LANG   this file uses the notation below. keep writing it in this notation.
FORM   RULE <topic> = <value>        hard rule
       RULE <topic> : <predicate>    hard rule, predicate form
       WHY  <reason>                 why the rule exists
       THEN <action>                 what to do when the rule triggers
       EX+  <good example>
       EX-  <bad example>
       REF  <path>                   where to look
       EXC  <exception>              where a rule is broken, and on whose word
READ   RULE is binding. WHY is context, not negotiable. EX is literal.

## Project

RULE product = ZYTerm, a desktop terminal for serial ports and the local console
REF  README.md
REF  ARCHITECTURE.md

RULE app.id = ru.styxheim.zyterm
WHY  the desktop matches a window to the program by one name, and a window
     naming no entry is drawn with the default icon and listed a second time
THEN the window `app_id`, the desktop entry, its `StartupWMClass` and the icon
     file all carry it; `main::APP_ID` and `ID` in the Makefile are where it is
     written
EX-  a window titled for a person used as the name a desktop matches

RULE lang = rust, edition 2024
RULE toolkit = egui and eframe

RULE targets = linux, windows
RULE platform_code : platform specific code is `cfg` gated
EX+  #[cfg(target_os = "linux")] fn text(&mut self) -> Option<String>
EX-  two source trees, one per platform

## Layout

RULE binary = the root package `zyterm`
RULE library_code = crates/*
RULE experiments = experiments/*, a workspace member each, none of them depended on
EXC  the slow line tests run the binary of `experiments/pty-throttle` as the
     command of a console (`RULE test.slow_line`); nothing links it, so no
     package depends on it
EXC  `experiments/glyphs`, `experiments/keys`, `experiments/flood`,
     `experiments/sixel`, `experiments/tmux-cc` and `experiments/pty-throttle`
     carry a `[[bin]]` outside the root package, by the user's word: each is a
     tool that answers a question — what the fonts of the toolkit carry, what
     the terminal sends for a key, what a terminal does as the bytes come
     faster, what a sixel payload holds and costs, what a real tmux says in
     control mode, what a terminal and a program do behind a slow serial line —
     and not a part of the program
EX-  a crate of `crates/*` carrying a `[[bin]]`
EXC  `crates/zyt-script` carries both, by the user's word: the engine is a
     library and the runner that drives a script with no window is how a
     script is tested at all

RULE crate.purpose : one crate is one purpose, builds and is tested on its own
THEN split a crate that grew a second purpose

RULE crate.deps : a library crate depends on no other library crate of this workspace
WHY  a crate that needs a sibling is not one purpose any more
THEN move the shared part into the binary, or make it its own crate both may use
EX+  zyt-term-egui -> zyt-term        the one allowed exception, it renders it
EX-  zyt-serial -> zyt-config
REF  ARCHITECTURE.md

RULE crate.knowledge : a library crate knows nothing of the interface, the configuration files or the other crates' types
EX+  zyt_term::ClipboardAccess, the crate's own type
EX+  plate-menu -> egui, a widget that knows the toolkit and nothing of this application
EX-  zyt_term::Terminal reading `settings.yaml`
EX-  a widget crate reading `settings.yaml` or holding text of its own

RULE crate.readme : every crate has a README.md naming its scope, boundaries and errors
REF  crates/*/README.md

RULE license = MIT, declared once in `[workspace.package]`
THEN every member says `license.workspace = true`; none carries a licence of
     its own
EX-  a crate declaring `license = "GPL-3.0-or-later"` and a `LICENSE` beside it

## Files

RULE file.max_lines = 1000
THEN split into submodules

RULE module.subject : one module is one subject
EX+  error.rs, params.rs, supervisor.rs
EX-  util.rs, helpers.rs, misc.rs

## Size

RULE size.goal : the least code that does the work; a feature is kept, its
     code is not
WHY  every line is read again by each agent and each person who works here
THEN `tools/inventory.sh size` before the work and `size <rev>` after it; the
     commit body names the growth of code when it is more than 100 lines
REF  docs/tools.md

RULE size.reuse : before new code, look for code that does the same
THEN `rg`, `ast-grep`, `tools/inventory.sh map`; extend what is there
EX-  a second buffer type in a second crate, written from the first

RULE size.clones : a change adds no clone of 100 tokens or more
THEN `tools/inventory.sh clones`; one subject in two places goes to one place
THEN two crates that need it: a crate of its own (`RULE crate.deps`)
EX-  the same setup of a test in two test files

RULE size.dead : code that no test runs is deleted or gets a test
THEN `tools/inventory.sh cover`; dead code is deleted in the same change that
     made it dead

RULE size.layer : no type, trait or function that only passes a call on
EX-  a trait with one implementation and one caller
EX-  a function that renames one call

RULE size.feature : a setting, an option or a mode is added only when the user
     asks for it
WHY  each one is code, a test, a text in two locales and a line of a document

RULE docs.public : every public item has a documentation comment
RULE docs.deny = `#![deny(missing_docs)]` in every library crate

RULE format = `cargo fmt --all`
WHY  the shape of the code is nobody's opinion, so it is nobody's to argue over,
     and a diff that is a reformatting is a diff nobody reads
THEN run it over every change, before the change is read or committed
EX-  a line laid out by hand that rustfmt lays out another way

RULE format.tool = rustfmt with its defaults, no `rustfmt.toml`
WHY  `cargo fmt` is the one that knows the members of the workspace; `rustfmt`
     on its own is handed files and reaches no further than the modules they
     name, so a crate nobody named stays unformatted
THEN `--all`, never a file at a time
REF  `cargo fmt --all --check` says whether it was run, and changes nothing

## Documentation files

RULE doc.files = README.md, ARCHITECTURE.md, docs/*.md, crates/*/README.md,
     docs/arch/showcase/shots/*.scheme
RULE doc.lang = english, plain and technical
RULE doc.plans = docs/plans.md, the work that is put off; a part that is
     built leaves it in the same commit

RULE text.lang = simplified english with no filler: short sentences, common
     words, one idea a sentence, nothing said for its sound
THEN applies to every text written into this repository: docs/ with the
     plans, the READMEs, the schemes, comments, messages of the log, commit
     messages
EXC  AGENTS.md: the notation of its head
EX+  The worker does not scan for a network port. "Found" means "connected".
EX-  It is worth noting that, in a sense, the worker could be said to refrain
     from scanning.

RULE doc.style : short sentences, one subject each; no metaphor and no prose
     that argues with itself
WHY  these files are read to find one answer, not from the first line to the
     last; a paragraph that circles its point is a paragraph nobody finishes
THEN say the thing, then the reason in one clause, then stop
EX+  A fatal error drops the handle at once: a held handle keeps the device
     node claimed.
EX-  A held handle is a handle that says a thing about the world that has
     stopped being true, and what comes of that is a device returning under a
     name nobody asked for.

RULE doc.shape : a list, a table or a diagram wherever one fits; prose for what
     none of them holds
WHY  a table of eight rows is read in a glance and the same eight sentences are
     not
THEN a mapping is a table, a sequence of states is a diagram, a set of rules is
     a list
EX+  | sequence | answered by |
EX+  Disconnected ──found + open ok──► Connected
EX-  eight sentences each naming one sequence and what answers it

RULE doc.diagram = plain text inside a fenced block, ascii and box drawing
     characters only
WHY  a diagram is read in a terminal and in a diff, and by an agent as text
EX-  an image file drawn by hand, a mermaid block, a diagram nothing in a
     terminal draws

RULE doc.picture : what a widget looks like is a picture its picture test
     rendered; how it behaves is a diagram
WHY  a rendered picture fails its test when the widget changes, so it cannot
     go out of date; a drawn one can
THEN `img/<name>.png` beside the crate README, named there by a label
     `[picture-<name>]: img/<name>.png`; `build.rs` writes the same labels as
     `data:` URLs before the README in `lib.rs`, because rustdoc copies no file
     and Markdown takes the first definition of a label
EX+  ![The parts of the menu][picture-parts]
EX-  a screenshot taken by hand, a picture with no test that renders it
REF  crates/plate-menu/README.md, crates/plate-menu/build.rs

RULE doc.settings : a documentation file never repeats what the settings page
     shows — a default, a unit, the entries of a list, the sentence of a hint
WHY  it is a second copy of the page, kept by hand, read by somebody who has the
     page in front of them, and it goes out of date in one of the two places
THEN name the setting and say what the page cannot: why it exists, what it
     costs, what breaks without it
EX+  The scrollback is a memory budget rather than a line count, because a row
     costs the full width of the window.
EX-  Sixteen mebibytes by default, which is some eight thousand lines at eighty
     columns.

RULE doc.numbers : a number that is a constant of the code is named, not
     written out
WHY  a number written twice goes out of date in one of the two places
EX+  `DEFAULT_READ_INTERVAL`
EX-  thirty-one milliseconds

RULE doc.subject : README is what the program does, ARCHITECTURE and its
     chapters are how it is built, a crate README is that crate's surface
THEN a decision with a reason goes to the chapter of its subject; a feature goes
     to README; what a caller has to know to use a crate goes to its README
REF  README.md, ARCHITECTURE.md, docs/

RULE doc.chapters : ARCHITECTURE.md is an index — the crates, the threads, the
     data path, the identity, the errors — and every other subject of it is a
     file of `docs/`
WHY  a documentation file is read for one answer and read whole; one subject is
     a hundred lines where the whole of it is a thousand
THEN write the chapter in `docs/<subject>.md`, name it in the table at the head
     of ARCHITECTURE.md, and leave in the index only what the chapters are read
     against
EX+  docs/ports.md, named in the table as the port state machine
EX-  a tenth section added to ARCHITECTURE.md
REF  ARCHITECTURE.md

RULE doc.once : one thing is written down in one file; the others name it and
     say where it is
WHY  two copies of a paragraph go out of date in one of the two places, and
     whoever reads both pays for both
THEN what a crate answers for goes to its README, what the application decides
     with it goes to the chapter, and the chapter names the README by path
EX+  The driver itself is `crates/zyt-serial/README.md`.
EX-  the table of platform calls standing in a chapter and in the crate README

RULE doc.crate : a crate README carries scope, boundaries and errors, in that
     order, and the signatures a caller starts from
REF  crates/plate-menu/README.md

RULE doc.current : a documentation file says what the code does now
WHY  a file that describes what was taken out is read as a description of what
     is there
THEN change the file with the code, in the same commit; delete what is gone
EX-  a paragraph about a control the status bar no longer has

RULE doc.schemes = docs/arch/showcase/shots/*.scheme, the schemes of threads,
     buffers and states agreed with the user on a desk of showcase
THEN a scheme agreed on a desk moves here, in english; the desk keeps no copy
THEN its first line names its subsystems and their paths:
     `# subsystems: <subsystem> (<paths>), ...`; a `note Subsystems:` says them
     on the page
THEN a view `Now` draws the code; a view `Plan` draws a plan file, named in
     the second line
THEN it changes with the code or the plan it draws, in the same commit
     (`RULE doc.current`); a view of a plan that was built becomes `Now`
THEN it is checked by `showcase scheme <file>`, never by a look at the page
THEN showcase shows the directory as the desk `zyterm-arch`, a link:
     `ln -s ../../zyterm/docs/arch/showcase /workspace/showcase/desks/zyterm-arch`;
     the notes of the desk are not tracked (`docs/arch/showcase/.gitignore`)

## Comments

RULE comment.place : comments document items with `///` and `//!`
RULE comment.body : no inline comments inside a function body
WHY  a line that needs an explanation needs a name instead
THEN name the value, or split the function

RULE comment.lang = english

## Errors

RULE error.enum : every crate defines one error enum in error.rs and a `Result<T>` alias

RULE error.shape : the enum is closed and machine readable, one variant per failure case, carrying what is needed to act on it

RULE error.source : an error of a lower layer is wrapped in a matching variant with `#[source]`
THEN add a variant instead of widening an existing one
EX+  #[error("instance slot error")] Instance { #[source] source: std::io::Error }
EX-  Err(format!("{error}"))

RULE error.message : error messages are english and diagnostic only, never shown to the user

RULE error.user_text : user facing text lives in the binary, in locales/app.yml, selected by `AppError::message_key`
REF  src/error.rs

## Text and language

RULE i18n = rust_i18n, the `t!` macro
RULE locales = en, ru
RULE locale.coverage : every key in locales/app.yml has an en and a ru value
WHY  a missing value would show the key to the user
THEN a test enforces it; run it
REF  tests/locales.rs

RULE script.resource : shell sent to a device lives in a file of its own, never
     in a string literal
WHY  a script is worth reading, and a Rust string literal is not where it reads best
THEN template it with `{holes}`, fold it to one line before sending
EX+  scripts/lib/fish/sh/list.sh, read at run time with `zyt.script.read_file`
EX-  a multi-line shell script inside format!()
REF  scripts/lib/fish/sh/README.md

RULE script.product : a transfer of this program is a Lua script under
     `scripts/`, read at run time and not built in
WHY  a transfer is the thing a person most often has to change about a device,
     and a change that needs a build is a change nobody makes
THEN ship it in `scripts/`, install it with `make install`, and let a copy in
     the configuration directory stand in for it
EX+  scripts/shell-transfer.lua
EX-  a protocol of the product written in Rust and reachable only by rebuilding
REF  docs/scripts.md
REF  scripts/README.md

RULE icon.glyph : an icon is a code point the fonts of egui carry, declared in src/ui/icons.rs
WHY  a code point no font carries is drawn as a box
THEN add it to `icons::ALL`; the test there asks the toolkit and fails if no font has it
EX+  pub const SETTINGS: &str = "\u{2699}";
EX-  "\u{2714}", a check mark no default font carries
REF  src/ui/icons.rs

RULE icon.search : an icon is picked from `cargo run -p glyphs`, never guessed at
WHY  the set the fonts of the toolkit carry is small and unobvious, and asking
     `Fonts::has_glyph` instead answers no for a code point whose first face is
     the one the replacement glyph comes from — it called the magnifier and the
     check mark missing while both are there
THEN read the list the tool writes and take a code point that stands in it, with
     the face that carries it
EX+  U+1F50D, the line of the list that says NotoEmoji-Regular carries it
EX-  dropping a code point because `has_glyph` said no
REF  experiments/glyphs/README.md

RULE log.lang = english, through the `log` crate

## Data path

RULE payload.transport = a swapped buffer (`ByteSwap`), whole chunks
WHY  one short lock and one pointer swap per portion, no allocation in steady
     state
EX-  a channel, a queue or a socket carrying payload bytes
REF  ARCHITECTURE.md, docs/data-path.md

RULE channel.content : a channel carries control information only
EX+  state changes, errors, progress
EX-  the bytes a device sent

RULE port.thread : the port is handled in its own thread
RULE ui.thread : the interface thread never blocks on the port

RULE screen.thread : the terminal and tmux belong to the thread of the screen
WHY  the terminal is fed as the bytes come, frames or not, and the window waits
     for nothing; a terminal is not `Send`, so it is made where it lives
THEN the window reads the board (`Session::content`, the values) and asks by
     `ScreenCommand`; a question is answered on a later frame with its
     `session::Purpose`
EX-  the window holding a `Terminal`, or waiting for an answer of the screen
REF  ARCHITECTURE.md, *Threads*

RULE run.stop : no clock ends a run in the window; Cancel asks the script,
     Force ends it at once
THEN a stop with nobody at the plate — a disconnect, another source, the
     window closing — is a Force; every forced end is a line in the log
REF  docs/scripts.md, *Stopping one*

RULE ui.name : every widget carries a name a screen reader can read
WHY  egui names a widget for accesskit by the text drawn on it: an icon is read
     as a code point, a switch or a value whose label stands beside it as
     nothing, and a hint shown on hover is not given to accesskit at all
THEN `ui::access::Named`: `tip` for an icon with a hint, `named` for the rest;
     a widget drawn by hand gives `Response::widget_info`
EX+  ui.button(icons::SEARCH).tip(&t!("command.search.open"))
EX+  widgets::switch(ui, &mut on, &t!("settings.status_bar"))
EX-  ui.button(icons::SEARCH).on_hover_text(t!("command.search.open"))
EXC  the terminal of `zyt-term-egui` gives accesskit nothing of what it shows,
     by the user's word: an accessible terminal is a goal of its own (README)
EXC  the number field egui draws inside a `Slider` has an identifier of its
     own that is not reachable from outside; the slider carries the name and
     the value
REF  docs/ui.md, *What a screen reader reads*

## Serial ports

RULE port.fatal : a fatally failed connection has its handle dropped at once
WHY  a held handle keeps the device node claimed, so a returning device appears under a new name
THEN drop the handle, then report

RULE port.identity = `PortId`: usb vendor, product, serial number
EX-  following a device by its path alone
REF  docs/ports.md

## Configuration

RULE config.dir = the platform configuration directory, resolved with the `directories` crate from `app.id`
RULE config.format = yaml through serde_yaml_ng
RULE config.write : a write goes through a temporary file and a rename
WHY  an interrupted write must not leave half a settings file

RULE config.unchanged : a file is written only when its content changes
THEN compare with the file on disk before every write
REF  crates/zyt-config/src/lib.rs, `ConfigStore::save`

RULE config.legacy : no code reads what an older version wrote
WHY  a shape that is read is a shape that is kept, so every one of them lives
     on in the types, the tests and the head of whoever reads them next; the
     files this program writes are its own, and it is the one that changes them
THEN change the file, the type and what writes it, and let the shape before it
     go; a file the current version cannot read is kept aside and the defaults
     are started from, which `start_over` already does
EX-  a field read under the name it used to have
EX-  an untagged enum whose second variant is what a file used to hold
EX-  a `legacy_*` field, a `migrate`, a line rewritten into the spelling of today
REF  src/main.rs, `start_over`

RULE config.default : a key a file does not carry reads as the default of that
     setting
WHY  a setting nobody set is not an older file; this is how a new setting
     reaches a file that was written without it
EX+  #[serde(default = "default_scrollback_memory")]

## Dependencies

RULE dep.version = the latest release on crates.io
EX-  a git or pre-release dependency

RULE dep.search : before writing code for a general problem, look for a crate that solves it

RULE dep.declare : versions are declared in `[workspace.dependencies]`
EX+  serde.workspace = true
EX-  serde = "1" in a crate manifest

RULE unix.calls = `rustix`, not `libc`
WHY  `rustix` gives a call of the system as a safe function with typed flags
     and an `Errno`; through `libc` the same call is `unsafe`, with raw
     descriptors and an integer to check by hand
THEN a call `rustix` lacks is looked for in another safe crate before `libc`:
     `signal-hook` for a signal handler
EX+  rustix::termios::tcsetattr(&fd, OptionalActions::Now, &termios)
EX-  unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) }
REF  experiments/pty-throttle/src/tty.rs

RULE unsafe : no `unsafe` where a safe wrapper or a safe shape of the code does
     the same
WHY  an `unsafe` block is a promise the compiler cannot check, kept by whoever
     reads it next
THEN look for a safe crate or another shape first; `unsafe` stays only where
     there is none — the Windows API through `windows`, `memmap2::Mmap::map` —
     the block as small as the call
EX+  the child of `pty-throttle` making its session itself with
     `rustix::process::setsid` before `exec`, rather than `pre_exec`
EX-  `unsafe { libc::kill(pid, libc::SIGHUP) }` where
     `rustix::process::kill_process` is

## Window tests

RULE test.window = `egui_kittest` drives the window of `zyterm`: a `Harness`
     over `App`, keys, text and clicks given as egui events, what is drawn read
     through the tree of accesskit
WHY  what a change does to the window is seen, not guessed, and seen again by
     every run of `cargo test`: with no display, no `sleep`, the same on every
     machine. A test of `Session` says nothing of a plate, a button or a frame
THEN a change to what the window shows or how it answers comes with a test of
     the window: open it, act on it, check the tree — a label, an entry
     enabled, what holds the keyboard — and what went to the line
THEN the test is a module of the binary under `src/`, since a binary has no
     library an integration test could reach; the configuration is a scratch
     directory, never one of a person
THEN `egui_kittest` with `eframe`, `wgpu` and `snapshot` is a dev-dependency of
     the root package
EX+  the menu of tmux: the button found by its label and clicked, `Right`,
     `Enter` on Rename…, a name typed, `rename-window -t @1 -- 'editor'` read
     from what was written
EX+  Home pressed in tmux control mode goes as `send-keys -t %0 -- 'Home'`
EX-  a step that waits by `sleep`: `harness.run()` until the window settles, or
     steps until a condition, with a bound
EX-  an element found by where it was drawn, when it has a label

RULE test.window.look : what a change does to how the window looks is looked at
THEN `harness.render()` to a PNG in a scratch directory, opened before saying
     what it shows; that PNG is not committed and is no reference
     (`RULE test.picture`)

RULE test.window.limits : `egui_kittest` hands egui its events, so what lies
     between the system and egui is outside it
THEN these are checked on a display (`RULE gui.display`) when a change touches
     them: a key from the layout through winit to `egui::Key`, the clipboard and
     the primary selection, input methods, files dropped, focus and size given by
     the desktop, the scale of the screen, the repaint woken from a thread, the
     identity of the window on the desktop, the tree accesskit hands the
     desktop (docs/ui.md, *What a screen reader reads*)

## Slow line tests

RULE test.slow_line : what goes over a line has a test behind
     `experiments/pty-throttle` at a slow speed — tmux in control mode, typed
     input, the flow of output, a transfer
WHY  a pseudo terminal answers at once and hides what a port shows: a reply cut
     across chunks, keys typed while a command is on its way, a screen drawn
     before its snapshot came; a fault seen only on a device is a fault found
     late
THEN the console of the test runs `pty-throttle --quiet -b <baud> -- <command>`
     at the slow end of a real port, 9600 or below; a wait is bounded by what the
     bytes cost at that speed, never by a `sleep`
THEN the test asks `$CARGO build -p pty-throttle --message-format=json` for
     the binary and takes the path cargo names; no directory of a build is
     assumed, so `cargo test` needs nothing done before it
THEN a test that cannot build it fails and shows why, it does not skip: a
     skipped test is one nobody reads
EX+  the menu of tmux opened at 9600 behind `tmux -CC new`: the notice and the
     lists come in pieces and refill it while it stands
EX+  Home typed into vim at 9600 in control mode moves the cursor
EX-  a fake device that answers at once standing for a slow line
EX-  `pv -L` in a pipe: the command sees a pipe, not a terminal
REF  experiments/pty-throttle/README.md

## Picture tests

RULE test.picture : a widget crate has picture tests; nothing else has
WHY  a widget is drawn the same way wherever it is used, so its pixels are its
     behaviour; the window of the application changes with every locale, theme
     and setting, and a picture of it would fail for reasons nobody changed
THEN the window is tested by its tree (`RULE test.window`), not by its pixels
EX+  `plate-menu`, `zyt-term-egui`
EX-  a picture of the main window of `zyterm` committed as a reference

RULE test.picture.tool = `egui_kittest` with `wgpu` and `snapshot`, a
     dev-dependency of the widget crate
THEN `tests/snapshots.rs`, one test a picture, output path `img`; a fixed
     window size and `Theme::Dark`, so a picture changes only with the widget
REF  crates/plate-menu/tests/snapshots.rs

RULE test.picture.renderer = lavapipe, the software Vulkan of mesa
WHY  the container has no GPU, and the same renderer draws the same pixels on
     every machine that runs it
THEN `mesa-vulkan-drivers` in `.devcontainer/Dockerfile.claude`; a machine
     without a Vulkan driver cannot run `cargo test --workspace`

RULE test.picture.update : a picture changes only together with the widget,
     and is looked at before it is committed
THEN `UPDATE_SNAPSHOTS=1 cargo test -p <crate> --test snapshots`, then open
     every PNG that changed; the `.new`, `.diff` and `.old` files a failed run
     leaves are ignored by git
EX-  a picture updated so a failing test passes, without opening it

## Checks before finishing

RULE checks : all six are clean before the work is reported done
THEN cargo fmt --all
     cargo clippy --workspace --all-targets
     cargo build --workspace
     cargo test --workspace
     cargo audit
     tools/inventory.sh deps

## Commits

RULE commit : finished work is committed, after the checks above are clean
THEN one commit per subject; the subject line says what the work does, and the
     body says what it does it for

RULE commit.clean : a message carries nothing of the machine it was written on
WHY  the history is read by people who were never on that machine, and it is
     read for as long as the repository lives; a secret written into it stays
     there after the file it came from is gone
THEN name the file, the type, the rule, the code point — the things the
     repository has
EX+  `src/ui/menu.rs`, `PlateMenu::layer_id`, `RULE icon.search`, U+1F50D
EX-  a session or request identifier, a service or instance identifier, a
     password, a token, a key, the name of a local user, a path under a home
     directory, the address of a machine
EX-  "fixed the crash seen in session 0f3a-... under /home/<user>/work"

## Tools

RULE tools = docs/tools.md, which tool to use for which task
THEN a new tool goes to `/opt/packages.sh` and to that file
RULE tools.read : a summary is read before a file; a file is opened only
     when the summary does not answer
EX+  `tools/inventory.sh map zyt-term`, then the one module it names
EX-  a whole file of 3000 lines read to find one function

RULE api.inspect : inspect a crate API instead of guessing it
THEN cargo brief, ruskel

RULE slow.line = `experiments/pty-throttle`, a command behind a serial line of
     a given speed
WHY  what the terminal and a program do on a slow line — tmux in control mode,
     a full screen program, a flood — is checked on a desk with no device on it
THEN run it in the console of the program, the command after `--`; it switches
     the terminal to raw mode, gives the command a session and a controlling
     terminal, follows the size of the window, and on the way out switches off
     the alternate screen, the mouse and the rest a command cut short left on
EX+  cargo run -p pty-throttle -- -b 9600 -- tmux -CC new
EX+  cargo run -p pty-throttle -- -b 115200 -f 7E1 -- vim
EX-  `pv -L` in a pipe: the program sees a pipe, not a terminal, so no raw
     mode, no size, no alternate screen
THEN a check made by hand this way becomes a test (`RULE test.slow_line`)
REF  experiments/pty-throttle/README.md

RULE gui.display : optional — the window run on a display with none attached:
     Xvfb for X11, cage in headless mode for Wayland
WHY  it is the one way to see what `RULE test.window.limits` names; for the
     rest a test of the window does the same, every run, and costs nothing to
     repeat
THEN used when a change touches what `RULE test.window.limits` names, or when
     the user asks; not required otherwise, and either display is enough unless
     the change is of one of them
THEN build, run the window, act on it, take a picture after each step and look
     at every picture before saying what it shows; the configuration is a
     scratch directory, never one of a person
RULE gui.display.x11 = display :88
WHY  :99 is a display of the desktop of the user, reachable from the container;
     a window opened there is on that desktop, and a picture taken of it shows
     that desktop
THEN X11, with no window manager, so the window is given the focus; the display
     and the window run in the background:
       Xvfb :88 -screen 0 1280x800x24 -nolisten tcp & sleep 1
       env -u WAYLAND_DISPLAY DISPLAY=:88 HOME=<scratch> XDG_CONFIG_HOME=<scratch>/cfg target/debug/zyterm &
       W=$(DISPLAY=:88 xdotool search --name . | head -1)
       DISPLAY=:88 xdotool windowfocus --sync $W
       DISPLAY=:88 xdotool type --delay 30 'echo hi'; DISPLAY=:88 xdotool key Return
       DISPLAY=:88 import -window root shot.png
THEN Wayland, as the user `headless`: cage refuses to run as root. Its runtime
     directory is that user's, mode 0700, and the program renders on the
     processor:
       runuser -u headless -- env -u DISPLAY XDG_RUNTIME_DIR=<run> HOME=<home> \
         WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
         cage -- /workspace/zyterm/target/debug/zyterm &
       runuser -u headless -- env XDG_RUNTIME_DIR=<run> WAYLAND_DISPLAY=wayland-0 \
         wtype -s 300 'echo hi' -k Return -s 300
       runuser -u headless -- env XDG_RUNTIME_DIR=<run> WAYLAND_DISPLAY=wayland-0 grim shot.png
THEN stop what was started by its name: `pkill -x zyterm`, `pkill -x cage`,
     `pkill -x Xvfb`, and the server of a tmux started for the check
EX+  wtype -s 300 -k Return -s 300
EX-  wtype -k Return: wtype is gone, and its keyboard with it, before the window
     takes the key
EX-  cage of the system packages: 0.1.4 has no virtual keyboard, so wtype types
     nothing into it; the one on the path is 0.1.5, built in the container
EX-  pkill -f zyterm: it matches the shell that runs it
REF  .devcontainer/Dockerfile.claude

## Exceptions

RULE exception : a rule is broken only when the user asks for it
