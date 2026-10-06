# Makes a file of no bytes, throwing away whatever stood there.
#
# A body is written chunk by chunk, each one seeking to where it belongs, and
# `dd` with `conv=notrunc` leaves what it did not write. The file therefore has
# to start empty, and this is what empties it.
: > {path} 2>/dev/null && \echo '##''# 000' || \echo '##''# 500'
