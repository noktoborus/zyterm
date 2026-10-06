# Scripts

What a transfer is, in this program. One directory here is one script: a
`Manifest.yaml` saying what it is called and what it needs, and the Lua it is
started from. The application reads the manifests and offers what they say
wherever a file is sent or taken — nothing of a script is run to list it.
`lib/` carries no manifest and is therefore no script: it is what `require`
reads.

| script | what it does | what it needs on the device |
| --- | --- | --- |
| `shell-transfer.lua` | carries files onto the device and back, in chunks, with the sums compared | a shell and nothing else |
| `shell-list.lua` | says what a directory of the device holds, carrying nothing | the same |
| `zmodem.lua` | the usual one: a name, a length, a resume | `rz` and `sz` |
| `ymodem.lua` | the same without the resume | `rb` and `sb` |
| `xmodem.lua` | the plainest, and it carries neither the name nor the length: what comes back is written into the path picked here and padded to a whole sector of 128 bytes | `rx` and `sx` |
| `cat-file.lua` | writes a file into the line with `cat` on the far end: believed, not checked | a shell |
| `shell-driven-scp.lua` | asks the device where it stands and what it holds, and lets `scp` carry the files over the network | a shell and a network |

The three modems tell the device first and start the program here once the
device has taken the command, whichever way the file goes: the program here
begins its handshake as soon as it starts, and a handshake arriving while the
shell of the device is still waiting for a command line is read as part of that
command line. What says the command was taken is the echo of a console; a line
that echoes nothing waits out `modem.DELAY` instead. Taking a file off the
device asks which file, because the program at the far end has to be told its
name and only the person at the console knows it.

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

send:
  target: file        # what the user picks before it runs
  finish: ""          # the key sent once the line is empty again

receive:
  target: directory
```

```lua
-- init.lua
return {
    send = function() end,       -- one function per direction the manifest offers
    receive = function() end,
    cleanup = function() end,    -- optional: what to do when it is stopped
}
```

`target` is `none`, `file`, `files`, `directory` or `directories`. `finish` is
`enter`, `ctrl+c`, `esc`, `\x03` or text. A direction the manifest leaves out
is one the script does not offer, and a direction it offers without a function
of that name is a script that is left out of the list with a word about why.

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
saving* in the settings turns it back on everywhere at once.

## Writing one

```sh
cargo run -p zyt-script -- list
cargo run -p zyt-script -- show shell-transfer
cargo run -p zyt-script -- check --all
cargo run -p zyt-script -- run shell-transfer --direction send \
    --target ./Cargo.toml --line pty --command sh \
    --answer remote=/tmp/here --answer digest=auto
```

`--line pty` is a shell under a pseudo terminal, which is what a console is;
`--line pipe` is a shell reading a pipe, which is what an `ssh` session is;
`--line stdio` makes the standard channels of the runner the line. `--answer`
fills a field of a dialog, so a window is needed for none of this.
