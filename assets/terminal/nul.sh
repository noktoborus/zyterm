#!/bin/sh
# NUL bytes on the screen.
#
# Run it in a console of ZYTerm. Every run of NUL bytes is drawn as one block: a
# red frame, the colours of the cell exchanged, the mark of a run and the count
# of the bytes it stood for.
#
# There is no setting for it. A NUL byte is what a standard terminal is told to
# drop, so a device that has gone quiet in the middle of a word leaves nothing
# behind anywhere else, and that nothing is what somebody watching a line came
# to see.
#
# The mark is U+2400, SYMBOL FOR NULL, wherever a font of the terminal carries
# it. None of the fonts the toolkit ships with does, so a window that chose no
# font draws the substitute instead — the window of numbers says which fonts
# those are, and the settings are where a font that carries U+2400 is chosen.

# nul <count>
nul() {
    head -c "$1" /dev/zero
}

printf 'one byte, between two letters: a'
nul 1
printf 'b\n'

printf 'a run of three: '
nul 3
printf ' and text after it\n'

printf 'the counts, one run per line:\n'
for count in 1 2 9 10 99 100 1000; do
    printf '%5d ' "$count"
    nul "$count"
    printf '\n'
done

printf '\ntwo runs with a letter between them: '
nul 4
printf 'x'
nul 4
printf '\n'

printf 'a run that reaches the end of the line, so the grid cuts the block:\n'
awk 'BEGIN { while (i++ < 78) printf "a" }'
nul 12
printf '\n'

printf '\ninside coloured text, to see the colours exchanged: '
printf '\033[33;44m'
printf 'yellow on blue '
nul 5
printf ' still yellow on blue'
printf '\033[0m\n'

printf 'over a selection, so drag the mouse across this one: '
nul 7
printf '\n'

printf '\nand where they come from: text of two bytes a character, read as one\n'
printf 'h\000e\000l\000l\000o\000\n'
