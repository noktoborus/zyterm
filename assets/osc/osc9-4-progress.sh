#!/bin/sh
# OSC 9;4: a program saying how far along it is.
#
# Run it in a console of ZYTerm. It plays every state the sequence has, in
# the order a real piece of work would meet them, slowly enough to be watched.
#
# The share is shown on the transfer button of the status bar, where it takes
# the place of the word "Transfer" for as long as it is reported. There is no
# transfer here — the button says what the console last reported, whoever the
# console is talking to.
#
# The five states:
#
#   9;4;0        nothing is running any more, and the word comes back
#   9;4;1;<n>    n of a hundred
#   9;4;2;<n>    something went wrong at n, and the share is marked
#   9;4;3        something runs, but how far along is not known
#   9;4;4;<n>    it stands still at n
#
# A share above a hundred is held at a hundred, and a state with no share is
# read as nought, so `9;4;1` alone is a beginning and not a silence.
#
# With Progress denied in the OSC support settings nothing is reported and the
# button keeps its word throughout.

# report <state> [share]
report() {
    if [ -n "$2" ]; then
        printf '\033]9;4;%s;%s\a' "$1" "$2"
    else
        printf '\033]9;4;%s\a' "$1"
    fi
}

# hold <seconds>
#
# `sleep` is the one thing here that is not a printf, and a fraction of a
# second is not POSIX, so whole ones are used and the walk is kept short.
hold() {
    sleep "$1"
}

printf 'watch the transfer button of the status bar.\n\n'

printf 'unknown: something runs, and nothing is said about how far\n'
report 3
hold 2

printf 'counting up, a fifth at a time\n'
for share in 0 20 40 60 80 100; do
    report 1 "$share"
    printf '  %s%%\n' "$share"
    hold 1
done

printf 'standing still at 100, which stays on the button\n'
report 4 100
hold 2

printf 'starting again, and failing at 40\n'
report 1 40
hold 1
report 2 40
hold 2

printf 'held at a hundred: 250 is reported, 100 is shown\n'
report 1 250
hold 2

printf 'no share at all, which is read as nought\n'
report 1
hold 2

printf '\ndone: nothing is running, and the button says the word again\n'
report 0
