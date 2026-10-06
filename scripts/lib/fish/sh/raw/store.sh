# One chunk written from the bytes themselves.
#
# The ready marker matters: the client must not send a byte before the device
# has switched the line, or the discipline still in force will chew it. The
# same command puts the line back into the mode a console is normally in, so a
# chunk that fails halfway cannot leave the console without an echo.
#
# The chunk is appended: chunks arrive in the order they stand in the file, so
# the end of it is where this one belongs. Seeking to the offset would cost
# `dd bs=1` a system call for every byte written, and measuring the file after
# every chunk would cost the file. It is measured once, when the last chunk has
# landed, by the command that asks for a size.
#
# `min 0 time 100` is what ends this command when the client stops sending. A
# raw line carries no signal — an interrupt is a byte like any other there — so
# a `head` waiting for bytes that will never come waits for ever, and it waits
# on a console with no echo that nothing can reach any more. With those two
# settings a read returns nothing after ten seconds of silence, which is the end
# of the file as far as `head` is concerned: it stops, the line goes back to the
# mode a console is normally in, and the device is usable again. A client that
# was killed mid-chunk therefore costs ten seconds and not the console.
#
# The line is put back with `min 1 time 0`, the settings of a console that reads
# a line at a time, because `-raw echo` alone would leave the timeout standing
# for whatever switches that line to binary next.
\stty raw -echo min 0 time 100;
\echo '##''# 001';
\head -c {count} >> {path} 2>/dev/null;
\stty -raw echo min 1 time 0;
\echo '';
\echo '##''# 200'
