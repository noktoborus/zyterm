#!/bin/sh
# OSC 8: hyperlinks, and the menus they open.
#
# Run it in a console of ZYTerm. It makes a small tree in a temporary directory
# and prints a link to each thing in it, so the menu of a file and the menu of a
# directory have something real to act on. An address of the network is printed
# too, because it opens a different menu.
#
# With OSC-8 denied in the OSC support settings, every line below is plain text.
# With the console marked unsafe, the addresses are links but are only copied,
# never opened.

root=${TMPDIR:-/tmp}/zyterm-osc8
mkdir -p "$root/a directory"
printf 'one\ntwo\nthree\n' > "$root/notes.txt"
printf '{ "name": "ZYTerm" }\n' > "$root/data.json"
head -c 4096 /dev/urandom > "$root/blob.bin" 2>/dev/null
printf 'inside\n' > "$root/a directory/inside.txt"

# link <uri> <text>
link() {
    printf '\033]8;;%s\033\\%s\033]8;;\033\\\n' "$1" "$2"
}

printf 'files:\n'
link "file://$root/notes.txt" "$root/notes.txt"
link "file://$root/data.json" "$root/data.json"
link "file://$root/blob.bin" "$root/blob.bin"

printf 'a name with a space, escaped as the address needs:\n'
link "file://$root/a%20directory/inside.txt" "$root/a directory/inside.txt"

printf 'directories:\n'
link "file://$root/a%20directory" "$root/a directory"
link "file://$root" "$root"

printf 'the network, and a link whose text is not its address:\n'
link "https://example.com/" "https://example.com/"
link "https://example.com/" "an ordinary link"

printf 'made in %s\n' "$root"
