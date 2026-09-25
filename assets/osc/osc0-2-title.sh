#!/bin/sh
# OSC 0 and OSC 2: the title a program gives the window.
#
# Run it in a console of ZYTerm. Both sequences say the same thing here — OSC 0
# names the window and its icon, OSC 2 only the window, and this terminal has
# one name to give either way.
#
# With Window title denied in the OSC support settings the window keeps its own
# name throughout, and nothing below shows anywhere but in this text.

# title <text>
title() {
    printf '\033]2;%s\007' "$1"
}

# hold <seconds>
hold() {
    sleep "$1"
}

printf 'watch the title of the window.\n\n'

printf 'OSC 2: a name of its own\n'
title 'a name from the console'
hold 2

printf 'OSC 0: the same, and the icon with it\n'
printf '\033]0;%s\007' 'named by OSC 0'
hold 2

printf 'a long one, which the window cuts to what it has room for\n'
title 'a title long enough that the window has to decide what of it to show, and it does'
hold 2

printf 'a name with a semicolon in it, which is not a separator here\n'
title 'work; and more of it'
hold 2

printf '\nempty: the window takes its own name back\n'
title ''
