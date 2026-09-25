# One chunk written from base64, in a here-document the shell reads itself.
#
# A shell reading its commands from a pipe reads ahead, and whatever it took
# would never reach a `head -c` waiting for the body. A here-document is read
# by the shell itself, which is the one reader that cannot be read past, so it
# is the one form that carries a body over a pipe as readily as over a console.
# The client writes the lines, closes them with the delimiter and then asks for
# the reply with `store-end.sh`.
#
# The chunk is appended. Chunks arrive in the order they stand in the file, so
# the end of the file is where this one belongs. Seeking to the offset would
# cost `dd bs=1` a system call for every byte written, and measuring the file
# after every chunk would cost the file: it is measured once, when the last
# chunk has landed, by the command that asks for a size.
#
# Nothing is done to the line here. The shell reads the body while it is still
# parsing this command, which is before any `stty` on the line could run.
\base64 -d << '{heredoc}' >> {path} 2>/dev/null
