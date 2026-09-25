#!/bin/sh
# OSC 52: store a text in the clipboard and read it back.
#
# Run it in a console of ZYTerm. The store is one sequence. The read is a
# question the terminal answers into the input of the console, so the script
# takes the terminal for that moment: without raw mode and without echo off the
# shell would read the answer as a command line and print it at the prompt.
#
# With the clipboard denied nothing is stored and nothing is answered, with copy
# only the store works and the read is refused with a notice in the terminal,
# and with copy and paste both work.

if ! command -v base64 >/dev/null 2>&1; then
    echo "base64 is needed to encode the text" >&2
    exit 1
fi

text=${1:-ZYTerm osc 52}
encoded=$(printf '%s' "$text" | base64 | tr -d '\n')

printf 'storing: %s\n' "$text"
printf '\033]52;c;%s\a' "$encoded"

if [ ! -t 0 ] || ! command -v stty >/dev/null 2>&1; then
    echo "no terminal on the input, the read is skipped" >&2
    exit 0
fi

printf 'reading it back\n'
saved=$(stty -g)
# min 0 time 20: a read returns as soon as the answer stops coming, after two
# seconds at the latest, which is also how a refusal is noticed.
stty raw -echo min 0 time 20
printf '\033]52;c;?\a'
answer=$(cat)
stty "$saved"

if [ -z "$answer" ]; then
    echo "no answer: reading the clipboard is not allowed"
    exit 0
fi

bel=$(printf '\007')
esc=$(printf '\033')
payload=${answer#*52;c;}
payload=${payload%%"$bel"*}
payload=${payload%%"$esc"*}

printf 'answered: %s\n' "$(printf '%s' "$payload" | base64 -d)"
