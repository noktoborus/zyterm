#!/bin/sh
# Random characters, to see what the terminal does with what it cannot show.
#
# Run it in a console of ZYTerm. Nothing here is meant to be readable: the point
# is that a window fed arbitrary characters keeps its grid, its scrollback and
# its cursor, and that a character no font of the terminal carries is drawn as
# the box of the toolkit rather than as a hole in the line. The settings are
# where another font is put in front of the chain to see one of them fill in.
#
# The blocks of runs of NUL bytes are `nul.sh`. This one leaves that byte out
# until the last section, so the two can be read apart.
#
# The escape byte is taken out of every section. What is being looked at is the
# decoder and the fonts; a random escape sequence would move the cursor, repaint
# the page and change the modes of the window, and then nothing on the screen
# would say which of the two had done it.
#
# Numbers here are decimal because awk does not read a hexadecimal literal, and
# every awk runs under LC_ALL=C so that `%c` writes the byte it was given rather
# than encoding it again.

# How many lines each section writes.
lines=8
# How wide a line is written.
width=78

# noesc — standard input with the escape byte taken out
noesc() {
    LC_ALL=C tr -d '\033'
}

printf 'printable ASCII, which every font carries:\n'
LC_ALL=C tr -dc ' -~' < /dev/urandom |
    head -c $((lines * width)) | fold -w "$width"
printf '\n'

printf '\nLatin-1 and Latin Extended-A, two bytes a character:\n'
printf 'the alphabets the fonts of the toolkit do carry.\n'
LC_ALL=C awk -v lines="$lines" -v width="$width" 'BEGIN {
    srand()
    # U+00A1 (161) .. U+017F (383), less the soft hyphen U+00AD (173).
    for (row = 0; row < lines; row++) {
        line = ""
        for (col = 0; col < width; col++) {
            point = 161 + int(rand() * (383 - 161))
            if (point == 173) point = 174
            line = line sprintf("%c%c", 192 + int(point / 64), 128 + point % 64)
        }
        print line
    }
}'

printf '\nthe Basic Multilingual Plane, three bytes a character:\n'
printf 'most of it no font of a terminal carries, so most of it is a box.\n'
LC_ALL=C awk -v lines="$lines" -v width="$width" 'BEGIN {
    srand()
    # U+0800 (2048) .. U+FFFF (65535), less the surrogates, which are no
    # characters and no terminal has to accept them.
    for (row = 0; row < lines; row++) {
        line = ""
        for (col = 0; col < width; col++) {
            do {
                point = 2048 + int(rand() * (65536 - 2048))
            } while (point >= 55296 && point <= 57343)
            line = line sprintf("%c%c%c", \
                224 + int(point / 4096), \
                128 + int(point / 64) % 64, \
                128 + point % 64)
        }
        print line
    }
}'

printf '\nbytes that are no UTF-8 at all:\n'
printf 'each of them is the replacement character, and the grid does not move.\n'
row=0
while [ "$row" -lt "$lines" ]; do
    head -c "$width" /dev/urandom | LC_ALL=C tr -d '\000\033'
    printf '\n'
    row=$((row + 1))
done

printf '\nand the whole of it together, NUL bytes included:\n'
printf 'the framed blocks are the runs of NUL; the rest is characters.\n'
row=0
while [ "$row" -lt "$lines" ]; do
    head -c "$width" /dev/urandom | noesc
    printf '\n'
    row=$((row + 1))
done

printf '\ndone. the grid, the scrollback and the cursor are where they were.\n'
