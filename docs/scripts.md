# Scripts

A transfer is a script. This chapter is how the application runs one: where
scripts are looked for, what a script is given, what happens to the line while
it runs, and how one is stopped. What a script may call is
`crates/zyt-script/README.md`; the scripts that ship with the program are
`scripts/README.md`, and the wire format of the shell transfer is
`scripts/PROTOCOL.md`.

## The engine

Lua 5.4, through `mlua`, one interpreter per run on a thread of its own. The
interpreter carries the tables of the host under one global, `zyt`, and nothing
else: `debug` is never loaded, and `os.execute`, `os.exit` and `io.popen` are
taken out. That is not about a hostile script — a script is a file of the
person using the program, like a transfer profile was. It is what makes two
promises keepable: the hook that stops a tight loop cannot be removed, and
every process a script started is in a list, so stopping it stops all of it.

Lua was picked over a pure Rust interpreter and over WebAssembly for one
reason: a script here is a text file somebody edits and runs. The cost is a
vendored C library in the build and `unsafe` in the dependency tree, and the
decision is the user's.

## One directory, one manifest

A script is a directory with `Manifest.yaml` in it and the Lua the manifest
names beside it. Starting the program reads the manifests and nothing else: a
list of what can be done is a question about files, and a program that ran
somebody's Lua to find out what it could do would have already done something.
Each manifest that is read is a line in the log, as is each one that is left
out and each copy that stands in for another.

```yaml
name: ZModem
order: 20
hold_line: true
entry: init.lua
variables: [remote_host]
send: { target: file, finish: "" }
receive: { target: directory }
```

The name is what a person reads — `ZModem`, `Shell Transfer`, `Cat file` — and
it is what the menus, the panel and the notices say. The name of the directory
is what everything else goes by: what an entry of a menu is addressed by, the
script a source remembers, the file its answers are kept in.

The Lua is loaded when a script is about to run, and when somebody asks whether
it could run at all (`zyt-script check`). A manifest offering a direction the
script has no function for is caught there and not on a device.

## Where a script may stand

Four directories, searched in this order; the first one carrying a name wins,
and the rest are remembered as shadowed.

| | directory of scripts | resolved by |
| --- | --- | --- |
| 1 | `scripts/` in the configuration directory | `ConfigStore::config_dir` |
| 2 | `scripts/` in the data directory of the user | `ConfigStore::data_dir` |
| 3 | `scripts/` in the shared directories of the system | `XDG_DATA_DIRS` through `xdg`, the known folder of shared application data on Windows |
| 4 | `scripts/` beside the executable | `std::env::current_exe`, and the tree a build stands in |

Each of them holds a directory per script, and the name of that directory is
the name the copies are matched by.

The order is the order of who decided, nearest first: what this person wrote,
then what they installed for themselves, then what was installed for the
machine, then what the program was shipped with. `make install` puts the
shipped scripts in 3, and the archive carries them in 4; the page of the
settings says which directory each script came from and what it stands in for,
because two copies of one name is the one thing that cannot be seen from the
name alone.

The fourth place is the directory of the executable, and a program standing in
a build directory also looks at the tree it was built from — `cargo run` leaves
the program in `target/debug`, and the scripts of the product are not copied
there.

`require` reads `lib/` of the same directories in the same order, so a script
of the configuration directory may take the library beside the program and
replace one file of it.

Listing what is installed loads every script and reads its manifest. Nothing of
a transfer may happen then, so the line and the dialog it is given refuse every
call while the clock and the processes do too: a script that works at the top
of its file fails there, in the list, instead of on a device.

## What a script is

The Lua answers with a table: a function per direction the manifest offers, and
an optional `cleanup`. Everything the application knows before it runs is the
manifest. `scripts/README.md` carries the shape of both.

## The line while a script runs

One script runs at a time and it owns the line. While `Session::script` stands
and the manifest says it holds the line, four things change, and they are the
same four a transfer program changed:

| | |
| --- | --- |
| the terminal | is not fed; the bytes of the device go to the script instead |
| the keyboard | writes nothing into the line |
| the answers the terminal owes a program | wait |
| the read interval | does not hold a chunk back: the protocol answers a block and waits for the next |

The script never touches the port. It writes into one buffer and reads from
another, and the application moves both once a frame — the same swapped buffer
the pipes of a transfer program used, so the data path costs one short lock and
one pointer swap per frame.

A script that says it does not hold the line is given one that is already shut:
what it writes there is refused rather than quietly dropped, and whatever it
does reaches the device only through the programs it starts.

The run ending is not the script ending. When the script is over the driver may
still hold bytes; the key the manifest names is sent once `pending_output` is
nothing, which is what a device waiting for a `ctrl+c` after a `cat` needs.

## Three ways of saying something

While a script holds the line the device is not drawn, so a script that said
nothing would be a window with nothing in it. There are three paths and they do
not mix:

| call | where it goes | what it is for |
| --- | --- | --- |
| `zyt.line.write` | the device | what the transfer carries |
| `zyt.term.write` | the terminal, as the device would be drawn | what the script shows a person, escape sequences and all |
| `zyt.notice.*` | the terminal, in the frame and colour of the application | the few lines that are about the run and not about the device |

`zyt.progress.*` is a fourth and it is not text: it writes `Session::progress`,
which until now only a program of the device could do through OSC 9;4.

## Asking something

A dialog is data. The script describes the fields and blocks on one call;
`src/ui/form.rs` draws that description and answers with a value per name. The
window does not move, what is behind it answers nothing, and the two ways out —
the button that gives up and `Esc` — are the same answer.

The window grows with what it asks: a form of three rows is three rows tall.
It stops at the window around it — the full width of it, and its height less
`form::ROOM_MARGIN` above and below, past which there is nothing to grow into —
and the rows scroll there, both ways,
because a value cut to the width is a value nobody can read. The hint above the
rows and the row that ends the window are outside what scrolls: they are what
it is answered with. That last row is also the one width the window is held to,
being laid out by what its widgets take, so a form of one short field cannot
squeeze two buttons into the width of that field.

Six kinds of field, and two decorations: one line of text, text of several
lines growing from one, a switch, one of several and one picked from a list —
both of them a button opening a `plate-menu` like every other list of this
program, and what an entry says about itself stands on the plate beside it —
any number of a list as boxes to tick, a line of text that asks nothing
and a line drawn across. One of several and one picked from a list differ in
one thing: the first always answers with one of them, the second may answer
with nothing.

Any number of a list is a column of boxes to tick, one entry a line: what a
row of a window lays out side by side is read across the window, and twenty
files read across a window is a line nobody follows. What an entry says about
itself is on the pointer there, the row of a list having no plate to put it on.

It is not a row of the grid either. The names in it are as long as whatever
named them, so a cell beside a label would be a column every other label is
measured against: the label stands above the list instead, and the list fills
the width of what stands above it, in a frame that says where it ends. The
fields on either side of one are a grid of their own.

A label of an ordinary row wraps rather than widening its column past
`form::LABEL_MOST` characters: a script says what a field is for in its label,
and a sentence kept to one line would push every field in the window to the
right of the longest of them.

The same description is answered from the command line — `zyt-script run
--answer FIELD=VALUE` — which is what makes a dialog testable with no window
anywhere. A required field nothing answers stops the run instead of waiting.

What a window looks like is another question, and `scripts/form-check` is the
answer to it: one form with every kind of field in it, carrying nothing and
holding no line, for opening after the drawing of them is changed. That it
still names every kind is asserted in `tests/scripts.rs`; how they are drawn is
read by looking.

### What is asked again

What a script was answered is kept per source, per script and per form, in
`forms/<source>-<script>-<form>.yaml` of the configuration directory: the chunk
size of a slow board is not the chunk size of a fast one, the directory on one
device is not the directory on another, and a script asking two things keeps two
answers that say nothing about each other. A form carries the name it is kept
under; one with no name of its own is kept under `forms::DEFAULT_FORM`.

Asking is what a form does until somebody says otherwise. The switch beside the
buttons is the only thing that stops it, and it is written down only when it is
turned off — `always_ask: false` and nothing at all are the two states a file
has, so a form nobody has decided about comes up. *Reset form saving* in the
settings takes that key out of every file; a file that never carried it is left
alone, because it already asks.

Three things bring the window back: the switch of that form, a form the script
marked `unsaved` — the answers of one are written nowhere, because what it asks
is about the device as it stands now — and a form the saved answers do not
answer, which is what a script that grew a field since the last time looks
like.

## Stopping one

A thread cannot be taken off a call it is inside, so stopping is a ladder.
`ScriptRun::tick` walks it once a frame, and the cancel button over the
terminal is the same button a transfer had.

```
cancel ─► the flag: every blocking call of the host refuses
       ─► the processes it started, by group, newest first
       ─► cleanup(), with a budget of CLEANUP_BUDGET
       ─► the hook of the interpreter raises, and keeps raising
       ─► past ABANDON_AFTER the thread is left and its line is shut
```

The flag is what lets go of a script waiting on a silent device. The hook is
what catches one that asks the host nothing, and it keeps raising because a
`pcall` would otherwise swallow the one refusal. The last rung is the one that
matters for the window: whatever the script is doing, the session is free of it
inside a known time. The thread is left where it is and its interpreter is
never dropped — one such state is the price of a script that answered neither —
and its line is shut, so nothing it does afterwards reaches a session that has
moved on.

Every process a script starts is in `ProcessRegistry`, in a group of its own on
unix and in a job object on Windows. The group is what reaches the program a
shell started; the job object is what reaches a program that detached itself,
which `taskkill /T` does not. Both are walked newest first, and the pipes are
closed with them, or a reading thread would stay behind on a `read`.

## What the shipped scripts needed that a profile could not say

The three things the profiles got wrong are things only a script can put
right, and they are worth naming because they are why this was worth doing.

A modem has to be started in an order: the device is told first and the program
here starts once the device has taken the command, because that program begins
its handshake the moment it starts and a handshake arriving while the shell is
still reading a command line is read as part of it. A profile said "wait 700 ms"
and hoped; a script waits for the echo of the console and falls back to a bound.

A modem taking a file off a device has to be told which file. The profile sent
a bare `sz`, which answers with its own usage; the script asks, because asking
is what it can do.

The third is where a transfer works. A profile carried a directory of the
device in its command line, so one profile was one directory and reaching
another prompt meant editing the settings. A script asks the line instead —
`pwd` — and works below what it answers; the one taking something off the
device lists what stands there and offers it, a directory among them travelling
with everything under it. No path of the device is a question any more, because
the answer is a command.

## Testing without the window

`zyt-script` is the same engine with no window: `list`, `show`, `check` and
`run`, with the line being the standard channels of the process, the pipes of a
program, a pseudo terminal, or nothing. A pipe is what an `ssh` session is like
and a pseudo terminal is what a console is like, and the shell transfer tells
them apart — raw needs a line `stty` can switch, and over a pipe it is refused.

The tests of the shipped scripts are `tests/fish_canned.rs` (the wire, against
replies written down beforehand), `tests/fish_shell.rs` (against a real shell,
over both) and `tests/scripts.rs` (the shape of every shell template, and that
every script loads and says what it is).

## What a leftover `profiles.yaml` means

Nothing. Transfer profiles are gone and nothing reads or writes that file any
more; a copy left in the configuration directory from an earlier version stays
where it is and is never looked at.
