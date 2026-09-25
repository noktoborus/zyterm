#!/bin/sh
# OSC 9: ask the desktop for a notification that carries a text only.
#
# Run it in a console of ZYTerm. Denied in the OSC support settings, nothing
# reaches the desktop.

body=${1:-ZYTerm osc 9}

printf 'asking for a notification: %s\n' "$body"
printf '\033]9;%s\a' "$body"
