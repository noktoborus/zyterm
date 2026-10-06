# What a directory holds.
#
# The names come from the shell itself, so nothing has to be installed for
# them and no column layout has to be guessed: an `ls` that prints the date
# differently, or omits the group, or is wrapped in an alias that adds a flag
# it does not know, would all have moved the name somewhere else. Only the
# size is still asked of `ls`, and a size that cannot be read is no loss —
# it says how full the bar is, nothing more.
#
# `[ -d ]` and `[ -e ]` follow symbolic links, which is the rule everywhere
# here: a link travels as what it leads to, and one that leads nowhere is
# left out. The change of directory is in a subshell, so the shell of the
# device stays where it was.
(
    \cd {path} 2>/dev/null || exit 1;
    \echo '##''# 100';
    for n in * .*; do
        [ "$n" = . ] && continue;
        [ "$n" = .. ] && continue;
        [ -e "$n" ] || continue;
        if [ -d "$n" ]; then
            \echo Pd;
            \echo S0;
        else
            \echo P-;
            \ls -ln "$n" 2>/dev/null | ( \read a b c d s r; \echo "S$s" );
        fi;
        \echo ":$n";
    done;
    \echo '##''# 200';
) || \echo '##''# 500'
