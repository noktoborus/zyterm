#!/bin/sh
# OSC 7: the directory a program says it is working in.
#
# Run it in a console of ZYTerm. The directory becomes the one of the
# application, which is where the file dialog opens and where a transfer picks
# its file, so the effect is seen in the file dialog rather than on the screen.
#
# The sequence has no switch of its own: a directory is a name and reaches
# nothing outside the window.
#
# A shell reports it by itself when it is asked to; this plays what such a shell
# would send.

# report <path>
report() {
    printf '\033]7;file://%s%s\033\\' "$(hostname 2>/dev/null || echo localhost)" "$1"
}

root=${TMPDIR:-/tmp}/zyterm-osc7
mkdir -p "$root/a directory"
printf 'one\n' > "$root/a directory/inside.txt"

printf 'open the file dialog after each step to see where it starts.\n\n'

printf 'the temporary directory: %s\n' "$root"
report "$root"
sleep 2

printf 'a name with a space, escaped as the address needs\n'
report "$root/a%20directory"
sleep 2

printf 'a plain path, which is read as well as a file:// address\n'
printf '\033]7;%s\033\\' "$root"
sleep 2

printf '\nback to the home directory\n'
report "$HOME"
