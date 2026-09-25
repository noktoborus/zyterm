# OSC 133 for bash: make the shell itself mark its commands.
#
# This one is sourced, not run. Its neighbours in this directory play a cycle
# once to show what the marks do; this makes every command of a real bash
# session carry them, which is what fills the command history of ZYTerm.
#
#   . /usr/local/share/zyterm/osc/osc133-bash.sh
#
# Put that line at the end of ~/.bashrc to keep it, or paste it into a console
# to try it for that session alone. It can also be handed to a console profile
# as `bash --rcfile <this file>`, but then ~/.bashrc is not read at all, so the
# file would have to read it itself.
#
# What it marks, and why each one is needed:
#
#   133;A  the prompt starts here
#   133;B  the prompt ends and what is typed begins here
#   133;C  the command was entered and its output begins here
#   133;D  the command ended, with its exit code
#
# The line that was typed is in none of them: it is what the shell echoed
# between B and C, and that is where ZYTerm reads it from. So B has to sit at
# the very end of the prompt, where the cursor stands while you type, and C has
# to come after the newline that entering the command echoed.
#
# C comes from PS0, which bash expands exactly once for each command line it
# has read and is about to run. That is the same event C stands for, so there
# is nothing to arm and nothing to guess. The obvious alternative, a DEBUG
# trap, fires for every command the shell runs — including the ones in
# PROMPT_COMMAND, which every distribution puts something in, and including the
# ones bound to a key with `bind -x`, which is how fzf binds Ctrl-R. Each of
# those would spend the mark meant for the command the user typed, and that
# command would then carry none.
#
# Nothing here depends on ZYTerm. These are the marks every terminal that
# understands OSC 133 reads, so a shell set up this way is set up for all of
# them; a terminal that understands none of it prints nothing, because the
# sequences carry no text.

case $- in
*i*) ;;
*) return 0 2>/dev/null || exit 0 ;;
esac

if [ -z "${BASH_VERSION-}" ]; then
    printf 'osc133-bash.sh is for bash; this shell is something else\n' >&2
    return 1 2>/dev/null || exit 1
fi

if [ -n "${__zyt_marks_loaded-}" ]; then
    return 0
fi
__zyt_marks_loaded=1

# Runs before every prompt: closes the command that just ended and opens the
# prompt of the next one.
#
# It stands first in PROMPT_COMMAND so that `$?` is still the exit code of the
# command the user ran, and it hands that code back, so whatever else was in
# PROMPT_COMMAND — a prompt that colours itself by the last exit code, say —
# sees what it saw before.
#
# D is written only when a command really ran. Entering an empty line draws a
# new prompt without running anything, and a D there would close a command that
# never opened. Where the shell cannot say which it was, `__zyt_close_every_line`
# is set and every prompt closes what came before it.
__zyt_mark_prompt() {
    local status=$?

    if [ -n "${__zyt_ran+set}" ]; then
        printf '\033]133;D;%s\a' "$status"
        unset __zyt_ran
    fi

    printf '\033]133;A\a'

    if [ -n "${__zyt_close_every_line-}" ]; then
        __zyt_ran=
    fi

    return $status
}

# Runs from the DEBUG trap on a bash too old for PS0, which is every one before
# 4.4. It has the failings named at the top of this file, so it is armed as
# late as it can be — after everything else in PROMPT_COMMAND has run — and
# nothing but the command that was typed is left to spend it. A key bound with
# `bind -x` can still spend it there; PS0 is the reason not to do this.
__zyt_mark_command() {
    if [ -z "${__zyt_armed-}" ]; then
        return 0
    fi
    case "$BASH_COMMAND" in
    __zyt_mark_prompt*) return 0 ;;
    esac

    unset __zyt_armed
    __zyt_ran=1
    printf '\033]133;C\a'
}

__zyt_arm_command() {
    __zyt_armed=1
}

# PROMPT_COMMAND is a string in bash before 5.1 and may be an array from 5.1 on.
# Both are prepended to, because this has to see `$?` before anything else does.
if [ "${BASH_VERSINFO[0]}" -gt 5 ] ||
    { [ "${BASH_VERSINFO[0]}" -eq 5 ] && [ "${BASH_VERSINFO[1]}" -ge 1 ]; }; then
    PROMPT_COMMAND=(__zyt_mark_prompt "${PROMPT_COMMAND[@]}")
else
    PROMPT_COMMAND="__zyt_mark_prompt${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
fi

# C is added to the end of PS0 and never put in its place. PS0 belongs to
# whoever was there first: systemd writes the context of every command into it
# — OSC 3008, installed as /etc/profile.d/80-systemd-osc-context.sh and on by
# default since systemd 257 — and a line that replaced it would leave that
# session unaccounted for. The mark carries no text and moves no cursor, so it
# costs the prompt in front of it nothing.
#
# `${__zyt_ran:=}` is an assignment and not a value: it expands to nothing and
# leaves the variable set, which is how the next prompt knows a command ran.
# Bash expands it only while `promptvars` is on, which is the default; with it
# off the assignment would be printed instead, so there the prompt arms itself
# and D closes every line, empty or not.
if [ "${BASH_VERSINFO[0]}" -gt 4 ] ||
    { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; }; then
    case "${PS0-}" in
    *'133;C'*) ;;
    *)
        if shopt -q promptvars; then
            PS0="${PS0-}"'${__zyt_ran:=}\033]133;C\a'
        else
            PS0="${PS0-}"'\033]133;C\a'
            __zyt_close_every_line=1
        fi
        ;;
    esac
else
    trap '__zyt_mark_command' DEBUG
    PROMPT_COMMAND="${PROMPT_COMMAND}; __zyt_arm_command"
fi

# B goes at the very end of the prompt, wrapped in \[ \] so readline counts it
# as no width at all; without that the shell would think the prompt is longer
# than it is and every long line would be drawn one column off.
case "$PS1" in
*'\]133;B'*) ;;
*) PS1="${PS1}\[\033]133;B\a\]" ;;
esac
