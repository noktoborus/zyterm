# zyt-xfer

File transfer through external programs, over pipes or beside the line.

## Profiles

`TransferProfile` is a name, the `hold_line` flag and one `TransferCommands` per
direction.

```
hold_line = true    the program is the far end of a conversation on the device
                    console: it reads what the device sends and answers on the
                    same channel. The console is taken from the terminal and
                    handed to it, one at a time, and the device is given a
                    command of its own first.
hold_line = false   the program is not on the line. remote() and finish()
                    answer with nothing, it runs through JobRunner rather than
                    TransferJob, and any number of them run at once.
```

A profile that does not say reads as one on the line.

`TransferCommands` is two `CommandStep`s: `local`, run here by the shell of the
platform, and `remote`, typed into the console of the device. Each carries its
own `delay_ms`, counted from the start of the transfer, so the profile decides
which side goes first. An empty `local` line means the direction is not
available (`is_available`); an empty `remote` line means the device needs no
command.

### Placeholders

| form | meaning |
| --- | --- |
| `{>file}`, `{>directory}` | one path |
| `{>files}`, `{>directories}` | any number, each quoted, space separated |
| `{:filename}`, `{:stem}`, `{:suffix}` | parts of the name |
| `{name}` | a value the caller keeps under that name |
| `{}` | the program's own |

A name is letters, digits, hyphen and underscore, at least one
(`is_variable_name`), which is what tells the four kinds apart without a rule to
remember. Anything else in braces is left standing. `target_kind` says what the
caller must ask the user for, and every substituted value is quoted with
`quote_for_shell`.

`variables()` — on a line, a direction or a profile — names what it asks for, in
order and each once; `resolve` takes the values as a `BTreeMap`. A name that is
missing or empty is `XferError::UnsetVariable`: `user@:/tmp` is a destination
that would be acted on rather than refused, and the moment to say so is before
anything runs. Nothing here says where the values live.

### The finish key

`finish` of a direction is the key sent once the transfer is over: `esc`,
`enter`, `tab`, `ctrl+<char>`, the escapes `\r`, `\n`, `\t`, `\e`, `\xNN`, or
literal text; empty sends nothing. `finish_bytes` turns it into bytes,
`finish_label` into text for a message.

Every shipped profile that runs `sh-xfer` ends with `enter`: that program leaves
the shell of the device with a line to read, and the prompt comes back only once
it is sent.

### Shipped profiles

zmodem, xmodem, ymodem; `Shell Transfer`, which needs nothing on the device but
a shell and says `--mode base64` out loud so a line that can carry raw bytes can
be given them by editing it; `Cat file`, which sends `ctrl+c` when it is over;
and `SCP to remote PWD`, which carries no byte over the line — it holds it only
long enough to ask where the device stands (`sh-xfer pwd-exec`) and lets `scp`
copy over the network into that directory, asking the source for `remote_user`
and `remote_host`.

## Running

`TransferJob::start` runs one resolved line with pipes on stdin and stdout —
a profile on the line.

`JobRunner::start(title, line, notify)` runs one beside the line and answers
with a `JobId`: stdin closed, both output channels into a `.txt` of its own in
the temporary directory, nothing reaching the terminal. `cancel(id)`,
`cancel_all()`, `poll()` (what ended since the last call), `jobs()` (every
entry with its `Outcome` — `Done`, `Failed(code)`, `Cancelled` — and how long it
ran), `remove(id)` (which takes the output file with it).

## Data path

The job never opens the line. The caller keeps the port and pumps both
directions:

```
line ──feed(&[u8])──► program stdin
line ◄─take_output()── program stdout
     stderr ──► TransferEvent::Log      exit code ──► TransferEvent::Finished
```

Both payload directions move in whole chunks through a swapped buffer. A
transfer program is driven, not talked to: it reads no keys and draws nothing,
so nothing here hands it a terminal.

Cancelling stops the whole process group, not only the shell that started it,
so the program talking to the device cannot stay behind on the line. Windows
kills the tree with `taskkill`.

The output pipe is read to its end rather than until the program exits: a short
program is gone before its last chunk is read, and `is_finished` waits for the
pipe too. A cancelled job reports itself finished at once.

## Errors

`Result<T, XferError>`: `EmptyCommand`, `Spawn`, `MissingPipes`, `ThreadStart`,
`TargetMismatch`, `InvalidFinishKey`, `UnsetVariable`, `Log`, `Finished`,
`Kill`. I/O errors are wrapped with `#[source]`.
