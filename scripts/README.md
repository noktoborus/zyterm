# Scripts

What a transfer is, in this program. One directory here is one script: a
`Manifest.yaml` saying what it is called and what it needs, and the Lua it is
started from. The application reads the manifests and offers what they say
wherever a file is sent or taken — nothing of a script is run to list it.
`lib/` carries no manifest and is therefore no script: it is what `require`
reads.

| script | what it does | what it needs on the device |
| --- | --- | --- |
| `shell-transfer.lua` | carries files into the directory the device stands in and back out of it, in chunks, with the sums compared | a shell and nothing else |
| `shell-list.lua` | says what the device holds where it stands, carrying nothing | the same |
| `zmodem.lua` | the usual one: a name, a length, a choice of files; this side speaks it itself (`zyt.zmodem`) | `rz` and `sz` |
| `ymodem.lua` | a name, a length and a time, several files in a row, no resume and no choice; this side speaks it itself (`lib/xymodem.lua`) | `rb` and `sb` |
| `xmodem.lua` | the plainest, and it carries neither the name nor the length: what comes back is written into the path picked here and padded to a whole block; spoken here as well | `rx` and `sx` |
| `cat-file.lua` | writes a file into the line with `cat` on the far end: believed, not checked | a shell |
| `shell-driven-scp.lua` | asks the device where it stands and what it holds, and lets `scp` carry the files over the network | a shell and a network |
| `telnet.lua` | offers the line on a port of this machine: one telnet client types into the device and reads what it says, which the terminal shows as well; a second client is turned away | nothing |
| `form-check.lua` | carries nothing: one window with every kind of field in it, for looking at | nothing |

ZModem is spoken here and not by a program: nothing has to be installed on
this side, and the files the device offers can be taken one by one. What goes and
where it lands are asked first, with the dialog of the file system, the way
XModem and YModem ask them: sending takes any number of files, which go in one
batch, each with its name and its length — files dropped onto the window go the
same way without being asked for; taking files off the device takes the
directory they land in. Its forms come up every time with what was answered last filled in.
Sending names the command the device runs and whether to run it at all. Taking
files off the device names where the choice comes from: the listing of the directory the device's shell stands in, whose picked
files are added to the command, or whatever the command offers. For the
second, two passes run the command once to learn what it offers, skipping all
of it, ask which files are wanted — one file is taken without asking — and run
it again to take them. A file already in the directory is kept and the new one
lands beside it, unless the form says otherwise; resuming takes a shorter file
for the start of the one offered and fetches the rest, trusting the name and
the size, since nothing compares the bytes. A device resumes the same way when
told `rz -r`.

YModem and XModem are spoken here too, by `lib/xymodem.lua`, written from
Forsberg's description: blocks of 128 or 1024 bytes, a CRC or the old sum,
whichever the receiver asks for, and for YModem a block 0 naming each file.
They tell the device first and start speaking once the device has taken the
command, whichever way the file goes: a handshake arriving while the shell of
the device is still waiting for a command line is read as part of that command
line. What says the command was taken is the echo of a console; a line that
echoes nothing waits out `modem.DELAY` instead. Taking a file off the device
asks which file, because the program at the far end has to be told its name
and only the person at the console knows it. XModem sends blocks of 128 bytes
unless the form says 1K, because not every loader takes the longer ones;
YModem always may. YModem sends any number of files picked at once, in one
batch, each named in a block 0 of its own. A file coming back with YModem lands
beside one already there. Stopping either sends eight cancels, so the program on the device stops
too.

Two habits of `lrzsz` shape them. It empties its line as it leaves, which on a
pseudo terminal can take its last ACK with it, so after the end of a transfer
anything but an answer followed by quiet is the receiver gone, and the end is
not sent again — a second EOT would be a ctrl+d to its shell. And it answers
late under load, so what came and was not read is dropped before every block:
an old answer read as the answer to the next block puts every answer after it
one behind.

`modem.carry` is still there for a script of somebody's own that hands the line
to a program of this machine. What `lrzsz -vv` says of a file starting and of
its bytes becomes events; anything else it says is printed into the terminal.

Every protocol here says where it stands through `modem.report`: `zyt.zmodem`
is handed it as `on`, `lib/xymodem.lua` and `modem.carry` call it with the same
events (`crates/zyt-script/README.md`). It turns them into `zyt.progress`
stages, bytes and shares, so the three read the same on the plate of the run;
`modem.meter` is what keeps the bytes of a protocol spoken in Lua from being
said on every block. Telling the device what to run is the stage `telling`,
and the first of two ZModem passes, which only learns what is offered, is
`listing`.

No script asks for a path of the device. The line is asked where the device
stands and the transfer works below that: the person at the console is already
somewhere, and a directory typed into a form would be a second answer to where
that is. Taking something off the device therefore lists what it holds there
and asks which of it to take — a directory among them travels with everything
under it.

Where what is taken lands on this side is another question, and
`shell-driven-scp` asks it in the window that offers the files, with the dialog
of the file system behind one row of it, rather than in a dialog of its own
before: the listing is what says where it should go.

`scp` draws its progress only on a terminal, and it runs here over pipes, so
`lib/scp.lua` reads where it stands out of what it says at `-vv`, which is in
the arguments it is given by default: the line that starts each file —
`Sending file modes` and `Sink` of the old protocol with the length of the
file, `do_upload` and `do_download` of SFTP, whose length is the one of this
side or of the listing — and the bytes the far end took, which `ssh` says as
the window of the channel moves (`rcvd adjust` going, `sent adjust` coming).
The window the channel opens with is room and not bytes, and is not counted;
every `Executing:` is a connection of its own and counts from nothing. The
share lags by what the far end has not yet taken, and a file is whole when its
connection says `Transferred` or the next file starts. Below `-vv` nothing says
the bytes, so the run says nothing of where it stands and every line goes to
the terminal; at `-vv` the debugging lines go nowhere and the rest — a warning,
an error — goes to the terminal. `tests/data/scp/` is what a real `scp` said.

`telnet` carries no file, and its one entry is `receive`: the line comes to
this side and out through a port. It asks for an address and a port — `127.0.0.1`
and `2323` unless told otherwise — listens there (`zyt.net`) and holds the line
for whoever connects: what the client types goes to the device, and what the
device says goes to the client and into the terminal, the one place a script
holding the line shows anything. One client at a time; a second is told the
line is taken and let go, and a client leaving leaves the port listening for
the next. The run ends when it is stopped or the line goes, and the port and
the client are closed with it. Of the protocol it speaks what a client needs:
this end echoes and suppresses the go-ahead, so a client sends each key as it
is typed and shows nothing of its own; every other option is refused, the
commands are taken out of what reaches the device, a return reaches it as the
`CR` a keyboard sends, and an interrupt as `ctrl+c`. Its plate has no bar —
a line handed to a client is not on its way to an end — and says the speed
each way, what the device sent and what the client typed over the last second,
as it was counted (`zyt.progress.speed`).

`PROTOCOL.md` is the wire format of both: the reply lines, the commands, the
two modes a body travels in, and where this differs from the FISH of Midnight
Commander. `lib/fish/` is the library that speaks it, and `lib/fish/sh/` is
every command as the shell it is — one file each, folded into the one line a
wire takes before it is sent.

## Where a script may stand

Four places, searched in this order — nearest to you first. The first one
carrying a name wins, and `zyt-script list` says what the others shadow.

| | where |
| --- | --- |
| the configuration directory | `~/.config/zyterm/scripts`, which is where you write one |
| your data directory | `~/.local/share/zyterm/scripts`, roaming application data on Windows |
| the system | `/usr/local/share/zyterm/scripts`, `/usr/share/zyterm/scripts`, shared application data on Windows |
| beside the program | `scripts/` next to the executable, which is what an unpacked archive runs |

`make install` puts what is shipped in the system directory. A script of yours
stands in for one of the same name shipped with the program, and so does your
`lib/`, so a change to the protocol is one file copied and edited.

## What a script is

```
zmodem/
    Manifest.yaml
    init.lua
```

```yaml
# Manifest.yaml
name: ZModem          # the name a person reads; the directory is the rest
order: 20             # the smaller stands first in the menus
hold_line: true       # it takes the line of the session while it runs
entry: init.lua       # the Lua it is started from
variables:            # values the source answers for
  - remote_host

send:                 # the entries it has, and nothing about them
receive:
drop:                 # files dropped onto the window are handed to this one
```

```lua
-- init.lua
return {
    send = function()                -- one function per entry
        local paths = zyt.ui.pick{ kind = "files", title = "What goes" }
        if not paths then
            return
        end
        -- the form, then the transfer
    end,
    receive = function() end,
    drop = function()                -- zyt.target.paths() is what was dropped
    end,
    cleanup = function() end,        -- optional: what to do when it is stopped
}
```

The manifest names entries and nothing else. What a run carries the script
asks for with `zyt.ui.pick` — `file`, `files`, `directory`, `directories` or
`save` — before its form; a key it leaves the device with it sends itself
(`zyt.line.write_key`), as `cat-file` ends `cat` with `ctrl+c`, and again from
`cleanup` when the run is stopped. An entry the manifest
leaves out is one the script does not offer; one it names without a function
of that name, or with anything under it — `target` and `finish` of an older
manifest — is a script left out of the list, with the log saying why.

`require` reads the directory of the script first and then `lib/` of every
directory of scripts, so a script of several files finds them by name and the
library beside the others by the same `require`.

What a script may call is `crates/zyt-script/README.md`; the chapter of the
subject is `docs/scripts.md`.

## What is answered, and what is asked again

A script asks what it needs with a dialog of its own, and what it was answered
is kept for that connection, that script and that form —
`forms/<connection>-<script>-<form>` of the configuration directory. The next
run takes it and asks nothing, so the second file to the same board is one
press.

```lua
zyt.ui.ask{
    id = "options",     -- what its answers are kept under; "default" without one
    unsaved = false,    -- true for a form whose answers are never kept
    always = false,     -- true for a form that comes up every time, answers kept
    title = "...",
    fields = { ... },
}
```

A form of no `id` of its own is kept under `default`, so a script asking two
things that are both worth keeping gives each of them a name. A form marked
`unsaved` is written nowhere and has no switch: what it asks is about the device
as it stands now — a listing of what is there — and an answer about one moment
is no answer to the next.

The switch beside the buttons says to ask every time, and asking is what a form
nobody turned it off for does. It is kept with the answers, and *Reset form
saving* in the settings turns it back on everywhere at once. A form marked
`always` has no switch: it comes up every run, with what was answered last in
its fields, because what it asks is what the run does.

A field marked `fresh = true` keeps the value the script gives over the one
answered last time: something the run was handed is what this run is about.

## Writing one

```sh
cargo run -p zyt-script -- list
cargo run -p zyt-script -- show shell-transfer
cargo run -p zyt-script -- check --all
cargo run -p zyt-script -- run shell-transfer --direction send \
    --pick ./Cargo.toml --line pty --command sh \
    --answer digest=auto
```

`--line pty` is a shell under a pseudo terminal, which is what a console is;
`--line pipe` is a shell reading a pipe, which is what an `ssh` session is;
`--line stdio` makes the standard channels of the runner the line. `--answer`
fills a field of a dialog and `--pick` answers the dialog of the file system,
so a window is needed for none of this; `--direction drop --target PATH` is a
file dropped onto the window.
