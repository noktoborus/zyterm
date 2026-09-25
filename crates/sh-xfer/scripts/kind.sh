# What a path is.
#
# Both tests follow symbolic links, so a link that leads to a file is a file
# here. How big it is, is another question and another command: a listing of a
# file names it by the path it was asked for and a listing of a directory names
# its entries, so telling the two apart from a listing alone is guesswork.
if [ -d {path} ]; then
    \echo '##''# 100';
    \echo D;
elif [ -f {path} ]; then
    \echo '##''# 100';
    \echo F;
else
    \echo '##''# 100';
    \echo O;
fi;
\echo '##''# 200'
