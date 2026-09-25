# How many bytes a file holds.
#
# `ls -ln` is asked first because it costs nothing: the fifth field is the size
# wherever there is one. A device whose `ls` is wrapped in a function, or whose
# columns run differently, answers with something that is not a number — and
# then `wc -c` reads the file and counts, which always answers.
#
# When neither can say, the reply line is bare. Saying nothing is better than
# saying zero, which a reader would take for an empty file; the client decides
# what to do without a size, and with chunks it can do nothing.
if [ -f {path} ] && [ -r {path} ]; then
    s=`\ls -ln {path} 2>/dev/null | ( \read a b c d s r; \echo "$s" )`;
    case "$s" in
    '' | *[!0-9]*) s=`\wc -c < {path} 2>/dev/null | ( \read n r; \echo "$n" )`;;
    esac;
    case "$s" in
    *[!0-9]*) s=;;
    esac;
    \echo '##''# 100' $s;
    \echo '##''# 200';
else
    \echo '##''# 500';
fi
