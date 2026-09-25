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
EXC  `experiments/glyphs`, `experiments/keys` and `experiments/flood` carry a
     `[[bin]]` outside the root package, by the user's word: each is a tool that
     answers a question — what the fonts of the toolkit carry, what the terminal
     sends for a key, what a terminal does as the bytes come faster — and not a
     part of the program
EX-  a crate of `crates/*` carrying a `[[bin]]`
EXC  `crates/sh-xfer` carries both, by the user's word: the protocol is a
     library and the command line driving it is the point of the crate

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

RULE doc.files = README.md, ARCHITECTURE.md, crates/*/README.md
RULE doc.lang = english, plain and technical

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
EX-  an image file, a mermaid block, a diagram nothing in a terminal draws

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

RULE doc.subject : README is what the program does, ARCHITECTURE is how it is
     built, a crate README is that crate's surface
THEN a decision with a reason goes to ARCHITECTURE; a feature goes to README;
     what a caller has to know to use a crate goes to its README
REF  README.md, ARCHITECTURE.md

RULE doc.crate : a crate README carries scope, boundaries and errors, in that
     order, and the signatures a caller starts from
REF  crates/plate-menu/README.md

RULE doc.current : a documentation file says what the code does now
WHY  a file that describes what was taken out is read as a description of what
     is there
THEN change the file with the code, in the same commit; delete what is gone
EX-  a paragraph about a control the status bar no longer has

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

RULE script.resource : shell sent to a device lives in a file under the crate's `scripts/`, built in with `include_str!`
WHY  a script is worth reading, and a Rust string literal is not where it reads best
THEN template it with `{holes}`, fold it to one line before sending
EX+  crates/sh-xfer/scripts/list.sh
EX-  a multi-line shell script inside format!()
REF  crates/sh-xfer/src/script.rs
REF  crates/sh-xfer/scripts/README.md

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
WHY  one short lock and one pointer swap per frame, no allocation in steady state
EX-  a channel, a queue or a socket carrying payload bytes
REF  ARCHITECTURE.md

RULE channel.content : a channel carries control information only
EX+  state changes, errors, progress
EX-  the bytes a device sent

RULE port.thread : the port is handled in its own thread
RULE ui.thread : the interface thread never blocks on the port

## Serial ports

RULE port.fatal : a fatally failed connection has its handle dropped at once
WHY  a held handle keeps the device node claimed, so a returning device appears under a new name
THEN drop the handle, then report

RULE port.identity = `PortId`: usb vendor, product, serial number
EX-  following a device by its path alone

## Configuration

RULE config.dir = the platform configuration directory, resolved with the `directories` crate from `app.id`
RULE config.format = yaml through serde_yaml_ng
RULE config.write : a write goes through a temporary file and a rename
WHY  an interrupted write must not leave half a settings file

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

## Checks before finishing

RULE checks : all four are clean before the work is reported done
THEN cargo fmt --all
     cargo clippy --workspace --all-targets
     cargo test --workspace
     cargo audit

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

RULE api.inspect : inspect a crate API instead of guessing it
THEN cargo brief, ruskel

## Exceptions

RULE exception : a rule is broken only when the user asks for it
