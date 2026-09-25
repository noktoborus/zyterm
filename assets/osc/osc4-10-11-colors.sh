#!/bin/sh
# OSC 4, 10, 11 and 12: the colors a program paints the terminal in.
#
# Run it in a console of ZYTerm. OSC 4 paints over one entry of the 256 color
# table, OSC 10 and OSC 11 move the default pair of the screen, and OSC 12 names
# the cursor. Each has a sequence that gives the color back: OSC 104, 110, 111
# and 112.
#
# A color is written as `rgb:rr/gg/bb` or as `#rrggbb`, and nothing else: a
# color name is not read and leaves the entry as it stood.
#
# With Palette denied in the OSC support settings every line below is drawn in
# the colors of the theme, and giving them back changes nothing because nothing
# was taken.

# paint <index> <color>
paint() {
    printf '\033]4;%s;%s\007' "$1" "$2"
}

# swatch <index> <text>
swatch() {
    printf '  \033[38;5;%sm%s\033[0m\n' "$1" "$2"
}

# hold <seconds>
hold() {
    sleep "$1"
}

printf 'the sixteen of the palette, as the theme has them:\n'
for index in 1 2 3 4 9 10 11 12; do
    swatch "$index" "color $index"
done
hold 2

printf '\nOSC 4: painting over four of them\n'
paint 1 'rgb:ff/00/7f'
paint 2 'rgb:00/ff/7f'
paint 3 'rgb:ff/d0/00'
paint 4 'rgb:00/a0/ff'
for index in 1 2 3 4; do
    swatch "$index" "color $index, painted over"
done
hold 3

printf '\nthe same colors said the other way the terminal reads: #rrggbb and #rgb\n'
paint 1 '#8000ff'
paint 2 '#0f0'
for index in 1 2; do
    swatch "$index" "color $index, said another way"
done
hold 3

printf '\nOSC 104: giving them back, one and then the rest\n'
printf '\033]104;1\007'
swatch 1 'color 1, back to the theme'
hold 2
printf '\033]104\007'
for index in 2 3 4; do
    swatch "$index" "color $index, back to the theme"
done
hold 2

printf '\nOSC 10 and OSC 11: the default pair of the whole screen\n'
printf '\033]10;rgb:f0/f0/e0\007\033]11;rgb:20/20/30\007'
printf '  every cell that names no color of its own follows them\n'
hold 3

printf '\nOSC 12: the cursor\n'
printf '\033]12;rgb:ff/40/40\007'
printf '  the block under the input is drawn in it\n'
hold 3

printf '\nOSC 110, 111 and 112: the pair and the cursor given back\n'
printf '\033]110\007\033]111\007\033]112\007'
printf 'done: everything is the theme again\n'
