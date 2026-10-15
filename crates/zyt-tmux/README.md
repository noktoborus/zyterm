# zyt-tmux

The control mode protocol of tmux, `tmux -CC`.

## Scope

| item | what it does |
| --- | --- |
| `Splitter::split(&mut [u8], each)` | splits one chunk of a line into `Piece`s: `Plain` bytes for the terminal, `Entered`, `Control` bytes of the protocol, `Left` |
| `Splitter::inside()` | true between `Entered` and `Left` |
| `Splitter::reset()` | outside again, for a start the caller gave up on as no tmux |
| `Client::new(lossy)` | reads the protocol; `lossy` on a port, where a byte may be lost |
| `Client::feed(&mut [u8], &mut impl Sink)` | reads a chunk of `Control` bytes: output, replies, notifications |
| `Client::send(Command, &mut Vec<u8>)` | appends the line of a command and queues what its reply answers |
| `Client::send_line(&[Command], …)` | several commands as one line, joined by ` ; `: tmux puts no `%output` between their replies, so a snapshot agrees with itself |
| `Client::send_keys(PaneId, &[u8], …)` | appends input as `send-keys`, runs of `-l` and `-H` |
| `Client::send_input(PaneId, &[Keys], …)` | the same for bytes and keys by name (`Keys::Named("Home")`), in order |
| `Client::rename_window(WindowId, &str, …)` | appends `rename-window` with the name quoted |
| `Client::sent()`, `forget()`, `fit(bytes)` | what waits for its reply; drop it when the line was lost; the longest line held across chunks |
| `Sink` | `output`, `reply_line`, `reply_end`, `notice`: where what comes back goes |
| `Command`, `Part`, `Flow`, `MODES` | what a reply answers, ids only and `Copy` |
| `Notice` | every notification other than output, borrowing from the line |
| `Layout::parse(&[u8])` | the layout of `%layout-change` as a tree of `Content`, the checksum checked; `panes()`, `find(pane)` |
| `Modes::parse(&[u8])` | the reply of `Command::Display`, the fields of `MODES` |
| `PaneId`, `WindowId`, `SessionId` | `%1`, `@1`, `$1` |

### The splitter

The mode starts on `ESC P1000p` anywhere in the stream. It ends on `ESC \`
right after a line that starts with `%exit`, and nowhere else: from tmux 3.4 a
reply of `capture-pane -e` carries a raw `ESC \` after a hyperlink, and the
client of tmux writes the end only after `%exit`. Bytes that end a DCS in an
emulator — `0x9C`, `0x18`, `0x1A` — do not end the mode, so UTF-8 passes as it
is.

A sequence cut by the end of a chunk is held until the next chunk says what it
is: the head of `ESC P1000p`, or the `ESC` of the end. What is held is a prefix
of a constant, so the splitter owns no buffer, and a held head that turns out to
be something else is given back as `Plain` from that constant.

`Control` is writable, so the protocol is decoded where it stands, in the chunk
the caller owns. A piece borrows no longer than the call of `each` it is given
to.

### The client

tmux answers commands in order, so the queue of what was sent is the whole
matching: a reply with flags `1` takes the command at the head of the queue.
Every command is queued, the ones whose reply is of no use too — the empty
reply of `send-keys` included — since one left out moves every reply after it
onto the wrong command. The pair with flags `0` that opens the mode answers
nothing sent and comes as `Command::Opening`; its lines are the reason of a
start that failed, `no sessions`.

| what comes | where it goes |
| --- | --- |
| `%output`, `%extended-output` | `Sink::output`, the octal escapes decoded in place: four bytes become one, so the write never passes the read |
| a line between `%begin` and `%end` | `Sink::reply_line`, as tmux wrote it, a line that starts with `%` included |
| `%end`, `%error` | `Sink::reply_end`, `failed` on `%error` |
| any other notification | `Sink::notice`; a `%` word this crate does not know is `Notice::Unknown` |
| `%subscription-changed` of a window | `Notice::SubscriptionChanged`, its name, the window and the value; one of a session or a pane, with no window, is `Notice::Unknown` |

A line cut by the end of a chunk waits in the client, in a buffer kept and
reused; `fit` bounds it, and a line longer than that is dropped as
`MalformedLine`.

When a reply is longer than it can be, `Sink::reply_line` answers
`Reply::Broken`: the reply ends as failed, and its lines are thrown away until
its `%end`. On a port (`lossy`) a byte lost from `%end` would otherwise leave
the reply open for everything after it:

| on a port | then |
| --- | --- |
| a notification inside a reply, known by its whole head (`%output %3`, `%layout-change @0`) | the reply ends as failed and the notification is read |
| a `%begin` inside a reply | the reply ends as failed and the next starts |
| an `%end` with no reply open | its `%begin` was lost: the command at the head of the queue ends as failed |

`send_keys` cuts input into runs: printable text — ASCII and valid UTF-8 — by
`-l --` in single quotes (tmux reads the options after the quotes are gone, so
text that begins with `-` would be a flag), everything else by `-H` in hexadecimal: control bytes,
an escape sequence whole with its printable tail, bytes that are not UTF-8, and
a run of text of one byte. By `-l` the `ESC` of a sequence would reach tmux as
the key Escape. Each run is one command and one reply.

`send_input` writes a key by name as `send-keys -t %N -- 'Name'`, a run of its
own; tmux encodes it for the terminal of the pane. A name that is empty or holds
a control byte is not sent: a newline would end the command.

`rename-window` expands formats in its argument, so `#` is written `##`;
`send-keys -l` expands none.

### What the line is held to

Numbers and lengths come from the other end of a line, so each is bounded:

| from the line | held to |
| --- | --- |
| nesting of a layout | `MAX_DEPTH`; deeper is `MalformedLayout`, since the parser is a recursive descent and is not given the stack |
| a line held across chunks | `Client::fit` |
| `%P`, `@W`, `$S` | `u32` |
| the size of a pane in `Modes` | at least one cell |
| the cursor, the scroll region, the saved cursor in `Modes` | inside the pane; a region upside down is the whole screen |
| `alternate_saved` | `None` off the alternate screen: tmux writes `4294967295` for a pane never on it, and keeps the last cursor saved for one that left it |
| tab stops | inside the width of the pane |

A tab cannot part the fields of a format: tmux writes it as `_`. `MODES` and
the lists are parted by spaces, the field that may hold one standing last.

### Tests

`tests/data/` holds sessions of a real tmux, every version the container
builds, written by `cargo run -p tmux-cc -- record all`. `tests/recordings.rs`
replays each through the splitter and the client — at once, byte by byte,
in the chunks the pty gave, in chunks of random length — and every way gives
the same events, with no error and every reply ended. A recording is made
again by `record`, never edited by hand.

## Boundaries

No I/O, no threads, no dependency on another crate of the workspace. The crate
does not know which terminal a byte goes to: the caller decides.

## Errors

`Error`, one variant per failure. None of them ends control mode; leaving it is
the caller's. The splitter fails on nothing.

| error | when |
| --- | --- |
| `MalformedLine` | a line that is no notification and no line of a reply, or longer than `fit` allows: skipped |
| `UnexpectedReply` | a `%begin` with flags `1` and nothing sent: its reply is read and thrown away |
| `LayoutChecksum`, `MalformedLayout` | a layout that cannot be applied |
| `MalformedModes` | a reply of `display -p` that is not the fields of `MODES` |

`Client::feed` reads the whole chunk whatever it meets and returns the first
error of it.
