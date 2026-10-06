# The wire

Files transferred over SHell, as this crate speaks it.

The far end runs nothing of ours. It is a shell — `sh`, `ash`, `busybox sh` —
with the usual utilities, reached over whatever carries a console: a serial
line, a pseudo terminal, an `ssh` session. The client sends shell scripts and
reads what they print. That is the whole protocol.

It is the protocol Pavel Machek wrote for Midnight Commander in 1998, kept to
where it matters and changed where a console is not an `ssh` pipe. The
differences are named at the end.

## Replies

Every command ends by printing a reply line:

```
### DDD[ text]
```

`###` marks it, `DDD` is a three digit code, and whatever follows is for that
code to define.

**The marker is looked for anywhere in the line, not at the start of it.** A
shell writes its prompt without a newline after it, so the first line of an
answer arrives glued to it:

```
/root # ### 100
```

An answer anchored to the start of a line would never be seen there. This is
the one thing most likely to go wrong in an implementation, and it fails
quietly: the framing reply is missed, the next one is read in its place, and
the command looks as though it answered nothing.

That freedom is safe only because **the client never sends the marker itself**.
A console echoes back everything written to it, so a script saying
`echo '### 200'` would come back looking exactly like the answer it asks for.
The scripts therefore write it in two pieces:

```sh
echo '##''# 200'
```

which is `### 200` to the shell and nothing of the sort to anything reading the
line. Every `echo '### NNN'` written below is shorthand for that form.

Everything that is not a reply line is noise and is skipped: the echo of the
script, the prompt, the output of a command that had nothing to say. An answer
of several lines is framed — `### 100`, the lines, `### 200` — so the echo of
the script, which comes first, is never mistaken for the answer.

| Code | Means |
|---|---|
| `000` | done, and there is nothing more to read |
| `001` | the device is ready for what the client sends next |
| `100` | what follows is the answer, up to the next reply line |
| `200` | the answer ended |
| `500` | refused |

## Talking

The client writes a script as one line ending in a newline. A newline ends a
line for a console as readily as the carriage return a keyboard sends, and a
pipe knows nothing else, so a newline is what is used.

Every command word is written with a backslash before it — `\ls`, `\cat`,
`\base64`. A console is interactive, so it expands aliases, and a device whose
profile wraps `ls` in one that adds a flag its own `ls` does not know would
answer every listing with nothing at all. The backslash is plain quoting, so
the alias is not looked for and every POSIX shell reads it the same way. It
does not reach past an alias to a shell *function* of the same name, which is
why nothing here depends on a program where the shell itself can answer.

The echo of a console is left alone everywhere but in the one command that
carries a raw chunk into the device — where it is turned off because the line
has to go into binary anyway, and put back by that same command. Everything
else the client sends comes back and is skipped as noise. A client that dies in
between leaves the console without an echo for one chunk and no longer, because
the setting was never meant to outlive the command that changed it, and is not
kept anywhere that would let it.

## Commands

`P` below stands for a path, quoted for the shell: wrapped in single quotes,
with every single quote in it closed, escaped and opened again.

### Hello

The client picks a mode before it says anything, and the probe asks for what
that mode needs and nothing else.

base64:

```sh
echo '### 100'
echo x | base64 >/dev/null 2>&1 && echo Hbase64 || echo Ebase64
echo xy | tail -c +2 >/dev/null 2>&1 && echo Htail || echo Etail
echo x | head -c 1 >/dev/null 2>&1 && echo Hhead || echo Ehead
echo x | wc -c >/dev/null 2>&1 && echo Hwc || echo Ewc
echo '### 200'
```

raw:

```sh
echo '### 100'
echo xy | tail -c +2 >/dev/null 2>&1 && echo Htail || echo Etail
echo x | head -c 1 >/dev/null 2>&1 && echo Hhead || echo Ehead
echo x | wc -c >/dev/null 2>&1 && echo Hwc || echo Ewc
stty -g >/dev/null 2>&1 && echo Hstty || echo Estty
echo '### 200'
```

One line per command, `H` for have and `E` for the error of not having it.

**An `E` ends the conversation there, and the program it names is the
complaint.** The client keeps no list of what it asked for: what is missing is
what the device said `E` about. A transfer that cannot place a chunk is better
refused before the first byte than halfway through a file.

Every tool is **tried, not asked after**. `command -v base64` answers for what
stands in the path, which on a device whose utilities are applets of one binary
is a different question from whether `base64` works. `stty -g` is the right
probe for the same reason twice over: it says whether `stty` works *on this
line*, which on a pipe it does not, however installed it is. That is what makes
raw over a pipe an `Estty` and not a puzzle.

Nothing of the line is touched here. Turning the echo off for the length of the
conversation would leave a console without one if the client died, and the mode
that needs the line changed is the only one entitled to change it — inside the
command that carries the body, and for no longer.

### Whether a file can be checked

One command per sum, asked only when a sum is wanted:

```sh
echo '### 100'; echo x | sha256sum >/dev/null 2>&1 && echo Hsha256sum || echo Esha256sum; echo '### 200'
```

and the same for `sha1sum` and `md5sum`. An `E` here is no error in itself: it
only means a file cannot be checked that way afterwards. A client asked for any
sum tries them strongest first and carries on unchecked when the device has
none; a client asked for one by name and refused it carries nothing.

### Where the device stands

```sh
echo '### 100'; pwd; echo '### 200'
```

### What a path really is

```sh
if cd P 2>/dev/null; then echo '### 100'; pwd -P; echo '### 200'; else echo '### 500'; fi
```

`pwd -P` resolves every symbolic link, which is how a walk of the device knows
it has come back to where it was.

### What a path is

```sh
if [ -d P ]; then echo '### 100'; echo D
elif [ -f P ]; then echo '### 100'; echo F
else echo '### 100'; echo O; fi
echo '### 200'
```

`D` a directory, `F` a file, `O` neither. Both tests follow symbolic links, so
a link to a file is a file here.

### How big a file is

```sh
if [ -f P ] && [ -r P ]; then
    s=`ls -ln P | ( read p l u g s r; echo "$s" )`
    case "$s" in '' | *[!0-9]*) s=`wc -c < P`;; esac
    case "$s" in *[!0-9]*) s=;; esac
    echo "### 100 $s"
    echo '### 200'
else
    echo '### 500'
fi
```

A command of its own, asked before a file travels. It does not trust the layout
of `ls -l`: the fifth field is the size only where `ls` is what it claims to
be, and `wc -c` reads the file and counts when the answer is not a number.

**The size may be missing**, and then the reply line is bare. A body travels in
chunks and a chunk is cut from a size, so there is nothing to be done with a
file whose size nothing can say: the client says so and carries nothing.

### Listing

```sh
( cd P 2>/dev/null || exit 1
  echo '### 100'
  for n in * .*; do
      [ "$n" = . ] && continue
      [ "$n" = .. ] && continue
      [ -e "$n" ] || continue
      if [ -d "$n" ]; then echo Pd; echo S0
      else echo P-; ls -ln "$n" 2>/dev/null | ( read a b c d s r; echo "S$s" ); fi
      echo ":$n"
  done
  echo '### 200' ) || echo '### 500'
```

One entry is three lines, in this order:

```
P<d for a directory, - for a file>
S<size in bytes>
:<name>
```

The `:` line closes an entry.

**The names come from the shell, not from a program.** An earlier draft of this
read `ls -lLa` and took the name from the ninth field, which asks the far end
to have `ls`, to have `grep`, to print nine fields, to put the date in three of
them and to have nobody wrap it in an alias. Any one of those failing gave an
empty listing and no word about why. A shell that can glob can list.

Only the size is still asked of `ls`, and a size that cannot be read is no
loss: it fills the bar and nothing else. The size a file actually travels by is
the one on the `### 100` line of the read.

`[ -d ]` and `[ -e ]` follow symbolic links, so a link is what it leads to and
one that leads nowhere is left out. The change of directory is in a subshell:
the shell of the device must not be moved, or every relative path sent after
this would mean something else.

A name with a newline in it cannot be carried this way. Nothing else is
refused.

### Making a directory

```sh
mkdir -p P 2>/dev/null && echo '### 000' || echo '### 500'
```

### Reading a file

The size is asked for first, and then one command per chunk:

```sh
echo '### 100'
tail -c +<offset> P 2>/dev/null | head -c <count> | base64
echo '### 200'
```

Lines of base64 until `### 200`. A line of base64 can never be mistaken for a
reply line. Raw is the same command without the pipe, with the line switched to
binary around the `dd` and put back by the same line:

```sh
echo '### 100'
stty raw -echo
tail -c +<offset> P 2>/dev/null | head -c <count>
stty -raw echo; echo ''
echo '### 200'
```

The reply line is written before the line is switched, so it crosses a console
that is still in its usual mode. The client reads exactly `count` bytes and
then looks for the next reply.

`<offset>` is where the chunk starts, counted from one, which is what `tail -c
+N` counts in; `head -c <count>` stops where the chunk ends. Both read in
blocks of their own choosing, so a chunk costs the device the bytes it carries.
`dd bs=1 skip=<start>` would say the same thing and cost a system call for every
byte of it, which on a line with any latency is not what makes a transfer slow,
and on a slow device is.

### Writing a file

The file is emptied first:

```sh
: > P 2>/dev/null && echo '### 000' || echo '### 500'
```

and then one command per chunk, each seeking to where its bytes belong. Which
command it is depends on the mode, and on nothing the client learns on the way.

**base64**, in a here-document:

```sh
base64 -d << 'SHXFER_EOF' >> P 2>/dev/null
<lines of base64>
SHXFER_EOF
echo '### 200'
```

A shell reading its commands from a pipe reads ahead, so a `head -c` waiting
behind it would never see the body: the here-document is read by the shell
itself, which is the one reader that cannot be read past. It is therefore the
one form that crosses a pipe as readily as a console, which is why base64 uses
it on both. The client writes the lines, the delimiter and then the line asking
for the reply.

Nothing is done to the line here, and nothing can be: the shell reads the body
while it is still parsing the command, which is before any `stty` on that line
could run. A console echoes the body back, and the client skips it as the noise
it is. That is the price of one form that always works.

**raw**, with the line in binary and `head -c` counting the bytes out:

```sh
stty raw -echo min 0 time 100; echo '### 001'
head -c <count> >> P 2>/dev/null
stty -raw echo min 1 time 0; echo ''; echo '### 200'
```

`### 001` matters: the client must not send a byte before the device has
changed the line, or the discipline still in force will chew it. The line is
put back by the same command, so a transfer that fails halfway does not leave
the console without an echo.

`min 0 time 100` is what ends this command when the client stops sending. A raw
line carries no signal — an interrupt is a byte like any other there — so a
`head` waiting for bytes that will never come waits for ever, on a console with
no echo that nothing can reach any more. With those settings a read returns
nothing after ten seconds of silence, which is the end of the file as far as
`head` is concerned: it stops, the line goes back, and the device is usable
again. A client killed mid-chunk costs ten seconds and not the console; a chunk
that ended that way is short, and the size the client asks for at the end is
what says so. The line is put back with `min 1 time 0`, the settings of a
console that reads a line at a time, because `-raw echo` alone would leave the
timeout standing for whatever switches that line to binary next.

It is the storing direction alone that needs it: the device writes a chunk it
retrieves and reads nothing, so nothing there can wait.

`stty -raw echo` is the plain inverse of `stty raw -echo`, and it is written
out rather than saved: keeping the old settings in a variable of the device
would leave that variable behind, and a protocol that keeps no state on the far
end must not keep one there either.

`<count>` is the same hole in both, and it is the size of the body **on the
wire**: the bytes themselves for raw, the encoded lines for base64. base64 does
not use it — the delimiter ends its body — and that is the only difference in
what the two are given.

A chunk is appended: the file is emptied first and the chunks go in the order
they stand in it, so the end of the file is where the next one belongs.
Seeking to the offset with `dd bs=1 seek=<start>` would place the same bytes
and cost a system call for every one of them.

When the last chunk has landed the client asks `size` about the file, and a
file of another length says a chunk never arrived. It is asked once and not
after every chunk: a command that measures the file costs the file wherever
`wc -c` is not clever enough to stat it, and a transfer that measures after
every chunk grows slower the longer it runs. The client says the stage in its
log — what was written, what the device says stands there, and which of the two
it believed — and a client may be told not to ask at all, and then only a sum
has anything to say about the file.

Everything a chunk does must cost the same whatever has gone before it. `tail
-c +<offset>` seeks on a file that can be seeked, which is what makes reading
one cost the bytes it carries; an implementation that reads its way there
instead would do to reading what measuring after every chunk does to writing.

## The body of a file

A console is not an `ssh` pipe. Its line discipline turns carriage returns into
newlines, acts on `^C` and `^S`, and may not pass the eighth bit. Raw bytes
across it arrive wrong. There are therefore two modes, and a client picks one
before it says anything: the probe then asks for what that mode needs, rather
than asking what the device has and deciding afterwards.

base64 is the mode a client uses unless it is told otherwise. It costs a third
more bytes, and it is the one that arrives: switching the line to binary fixes
the discipline of the far end, and nothing else — not a modem, not a terminal
server, not a port setting in between that eats a byte of its own. It is also
the only one that crosses a pipe at all, because a pipe has no line to switch.

The bytes are cut into lines of **57**, which is **76 characters** encoded —
what `base64` itself writes. This is not a matter of taste. A terminal in its
usual mode holds one line and no more, and what does not fit **is dropped where
it stands**: no error, no gap, no shorter read. The reader simply never sees
those characters, `base64 -d` decodes what did arrive, and the file is wrong
with nothing to say so. POSIX promises 255 characters; Linux keeps 4096. A line
of 2048 fits Linux and not the promise, which is how a transfer that works on a
pipe fails over `ssh` to something else.

The same limit applies to the commands, and they are not all within the
promise: the probe runs to about 440 characters and the listing to about 340.
Every device this has met keeps 4096, so they pass; a device that keeps only
the 255 POSIX asks for would chop them, and the answer would be a protocol
error rather than a quietly wrong file. Splitting them would cost a round trip
apiece, which on a slow line is worse than the risk.

### How big a chunk is

The chunk size is the number of bytes **on the wire**, and the client says what
it is. The slice of the file it stands for follows from the mode: raw carries
the chunk itself, and base64 carries as many whole lines of 57 bytes as fit —
`lines = chunk / 77`, at least one, because a line is 76 characters and a
newline. A small chunk says where a transfer stands more often and costs a
round trip more often for it.

## What a walk carries

A symbolic link is never carried as a link: it is followed and what it leads to
is carried in its place. That is what `[ -f ]` and `[ -d ]` already
do, and it is what someone copying a tree onto a board expects.

A link that leads back into its own tree would make the walk endless. On this
machine the walker reports it and the walk stops with a complaint naming the
path. On the device the real path of every directory is asked for with `pwd -P`
and remembered, so a directory reached twice is walked once; a walk deeper than
sixty four directories is refused as well, for a device whose `pwd -P` cannot
answer.

## Where this differs from Midnight Commander

- MC sends `#COMMAND arg` as a comment line before the script. Since 4.8.31 it
  sends scripts only, and so does this: the comment was never read by anything.
- MC carries the body raw and counts it out with `dd bs=4096 count=N` plus a
  remainder, and a file is one command. Here a file is a command per chunk,
  each one naming the offset it belongs at, so a transfer that stops says where
  it stopped and can be picked up from there.
- MC's listing carries the date and the owner. Nothing here needs them.
- The size of a file is a command of its own, and it rides on the `### 100`
  line of that command instead of standing on a line before a body. A reply
  line is then the only thing the client has to recognise, which is what makes
  the echo of a console harmless.
- The echo of a console is turned off for the length of the command that
  carries a raw chunk, which MC never does because an `ssh` pipe has none.
- Codes `000`, `001`, `100`, `200` and `500` are used; the rest of the FTP
  superset MC inherits is not.
