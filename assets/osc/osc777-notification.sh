#!/bin/sh
# OSC 777: ask the desktop for a notification that carries a heading and a text.
#
# Run it in a console of ZYTerm. It is a switch of its own, so OSC 9 may be
# denied while this one is allowed, and the other way round.

title=${1:-ZYTerm}
body=${2:-osc 777}

printf 'asking for a notification: %s / %s\n' "$title" "$body"
printf '\033]777;notify;%s;%s\a' "$title" "$body"
