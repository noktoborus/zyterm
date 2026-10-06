# One chunk of a file, as the bytes themselves.
#
# The reply line is written before the line is switched, so it crosses a
# console that is still in its usual mode. The same command puts the line back
# into the mode a console is normally in, so a chunk that fails halfway cannot
# leave the console without an echo.
#
# The chunk is cut with `tail -c +{offset}` and `head -c {count}`, which read
# the file in blocks rather than a byte at a time.
\echo '##''# 100';
\stty raw -echo;
\tail -c +{offset} {path} 2>/dev/null | \head -c {count};
\stty -raw echo;
\echo '';
\echo '##''# 200'
