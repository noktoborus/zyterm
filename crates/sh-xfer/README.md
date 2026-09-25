# sh-xfer

Files transferred over SHell: a file onto a device that runs nothing but a
shell, and back.

## Scope

`PROTOCOL.md` is the wire format. The far end runs no program of ours: every
command is a shell script and every answer is what it printed.

The scripts live in `scripts/`, one file per command, built in with
`include_str!` and folded to one line before they go. Shared commands sit at the
top; the four a mode owns sit in `scripts/base64/` and `scripts/raw/`.
`scripts/README.md` names each file, what it asks and what fills its holes.

- `Command` is one script with its holes filled; `ModeCatalog` is a whole set,
  built by `ModeCatalog::base64()` or `::raw()`. `Mode` is settled when the
  session is made, not learned on the way.
- `Session::new(reader, writer)` is the conversation, generic over the two
  halves of the line, so a pair of buffers in a test drives it as readily as a
  console: `hello`, `offers_digest`, `pwd`, `canonical`, `kind`, `size`, `list`,
  `digest`, `make_directory`, `create`, `retrieve`, `store`.
- `Reader` is the far half with a deadline: a console has no end of file, so a
  plain read on a device that stopped answering would wait for ever.
- A file travels in chunks carrying the offset they belong at. `set_chunk_size`
  is bytes of the *line*; `slice` says what that is in bytes of the file. The
  next command is worked out from the bytes that really moved.
- `Report`, set with `listen`, hears every command and every chunk. The library
  says what happened; how it reads belongs to whoever drives it.
- `local_items` / `remote_items` turn named paths into the files under them and
  where each goes. A symbolic link is followed and what it leads to is carried
  in its place; a link back into its own tree is caught on both sides.

### Modes

| mode | how the body travels | needs on the device |
| --- | --- | --- |
| `base64` | a here-document the shell reads itself | `base64`, `tail`, `head`, `wc` |
| `raw` | counted out with `head -c`, line switched to binary | `tail`, `head`, `wc`, `stty` |

base64 is the one form that crosses a pipe as readily as a console: a shell
reading commands from a pipe reads ahead, and a reader behind it would never see
the body. Raw is a third faster but gets through only when nothing between the
two ends touches the bytes, which a console usually does — a pipe answers
`Estty` and carries nothing raw.

Raw switches the line with `stty` *inside the command that carries the body and
for no longer*, with `min 0 time 100`, so a client that stopped sending ends
that command after ten seconds instead of leaving a console without an echo, and
puts it back with `stty -raw echo min 1 time 0` rather than from a variable of
the device.

`hello` asks whether the device has what the mode needs by trying each command.
The probe answers `H<program>` or `E<program>`, and an `E` ends the conversation
naming that program — the client keeps no list of what it asked for.

### Digests

`Digest` is `Md5`, `Sha1` or `Sha256`, each with a probe of its own, so the
device is asked for the one that is wanted and nothing else. `--digest auto`
tries them strongest first and carries on unchecked when the device has none;
one named outright is refused rather than carried unchecked. All three are
computed here rather than run, so a machine without `sha1sum` still checks what
it sent.

### `pwd-exec`

The one command that starts a program of this machine. It asks the device where
it stands, puts the answer where `{}` stands in the arguments and runs it. The
library opens no process — that is `src/exec.rs` of the binary.

## The command line

```sh
sh-xfer get PATH...       # from the device to here
sh-xfer get --all         # everything the remote directory holds
sh-xfer put PATH...       # from here to the device
sh-xfer list [PATH]       # what the device has there, carrying nothing
sh-xfer pwd-exec PROGRAM -- ARG...   # run a program of this machine where the
                                     # device stands
```

```
-C, --directory <DIR>          directory of this machine
-r, --remote-directory <DIR>   directory of the device
    --mode <base64|raw>        base64 by default
    --digest <none|auto|md5sum|sha1sum|sha256sum>   none by default
    --chunk-size <BYTES>       bytes of the line to a chunk; unsaid, 2048 over
                               a console and 65536 over a pipe
    --size-check <ON|OFF>      ask how long a file is once written, on by default
-v, --verbose                  say the commands, not only the progress
    --timeout <SECONDS>        how long to wait for the device, 30 by default
    --all                      carry the whole remote directory
-q, --quiet                    say nothing but what went wrong
```

**Standard input and standard output are the line**; everything said to the user
goes to standard error. That is what lets it be driven the way `sz` and `rz`
are, by a terminal that owns the line:

```sh
sh-xfer put ./firmware.bin -r /tmp
```

A line per chunk:

```
[3/12] send firmware.bin 2964:4446 size 1482... OK 43% 0.42s
```

The `OK`, the share and the time are written once the next chunk has been worked
out from what really moved. The time says whether the line is slow or the device
is; a share creeping up says nothing about which chunk cost what.

`get` names every file that arrived, each written as an OSC 8 hyperlink to the
path it was saved at, so a terminal that draws links opens it from the line that
says it arrived. The address is a `file://` one, made absolute without asking
the file system.

`pwd-exec` sends no probe — a device that has none of `base64`, `tail`, `head`
or `wc` still knows its own directory — and opens no transfer. The program it
starts gets no line: stdin closed, both its channels to standard error, because
standard output here is the line and a byte written there would be read as a
command. A program that would ask a question fails rather than wait for an
answer nobody can give, so what is run this way runs with keys or with nothing
to ask. Its exit code becomes the exit code of `sh-xfer`; a signal counts as a
failure.

```sh
sh-xfer pwd-exec scp -- -v -O ./firmware.bin root@192.168.1.1:{}
```

`list` needs no terminal and carries nothing: it answers what a device actually
says when a listing comes back empty, with `RUST_LOG=debug` showing the raw
lines beside it.

## Boundaries

No line is opened here and no process is started by the library: the caller
hands in a reader and a writer. Nothing is printed for the user by the library —
the binary does that. The crate knows nothing of ZYTerm, which drives it as
one transfer profile among others.

## Errors

`Result<T, ShXferError>`:

| variant | when |
| --- | --- |
| `Quiet` | the device stopped answering |
| `Refused`, `Unexpected` | a reply that was not the one the command wanted |
| `Truncated`, `NoSize` | a file arrived short, or nothing could say its size |
| `MissingCommand` | the probe named something the mode or digest needs |
| `NothingToTake` | `get` was named neither a path nor `--all` |
| `NoWorkingDirectory` | the device did not say where it stands |
| `NoProgram` | the program `pwd-exec` was given could not be started |
| `Decode`, `Loop`, `Walk`, `Io`, `Line` | the rest |
