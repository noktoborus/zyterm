#!/bin/sh
# OSC 133: the marks of a shell, and the command history they fill.
#
# Run it in a console of ZYTerm. It plays a whole shell cycle around a handful
# of real commands — prompt, the line as a shell would echo it, the output, the
# exit code — so the history has something in it without a shell that speaks
# OSC 133 being installed.
#
# What to look for afterwards:
#
#   - the turning arrow left of the gear, which appears once there is a history
#     and is not drawn while there is none;
#   - ctrl+shift+r, which opens the same list;
#   - choosing an entry types it into the shell without a newline, so it stands
#     there to be read once more, and moves it to the top of the list;
#   - `ls` is played twice on purpose: it is kept once, at the top;
#   - `false` fails on purpose and is kept like any other. A command is written
#     down when its output starts, which is before anything is known about how
#     it ended, so what it exits with plays no part — and a command that failed
#     is exactly the one somebody wants to fetch back and correct.
#
# The list lives in `history/` of the configuration directory, one file per
# source, and it is read again on every look — so a second copy of ZYTerm on
# the same console adds to the same list.
#
# With OSC-133 denied in the OSC support settings nothing is written at all, and
# the button stays away.

# mark <letter> [argument]
mark() {
    if [ -n "$2" ]; then
        printf '\033]133;%s;%s\a' "$1" "$2"
    else
        printf '\033]133;%s\a' "$1"
    fi
}

# play <command line>
#
# The command text is in none of the sequences: it is what the shell echoes
# between B and C, and that is where ZYTerm reads it from. So the line is
# printed, exactly once, in between the two marks.
play() {
    mark A
    printf '%s$ ' "${PWD:-~}"
    mark B
    printf '%s\r\n' "$1"
    mark C
    sh -c "$1"
    code=$?
    mark D "$code"
    return 0
}

play 'echo the history is filled by the marks of a shell'
play 'ls'
play 'uname -sr'
play 'printf "%s\\n" one two three'
play 'ls'
play 'false'

mark A
printf '%s$ ' "${PWD:-~}"
printf '\r\n'
printf 'six commands were played, five of them are kept: ls was played twice.\n'
printf 'open them with ctrl+shift+r, or with the arrow left of the gear.\n'
