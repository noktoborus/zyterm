# What a program of the device says the sum of a file is.
#
# Only the first field of whatever it wrote is taken, which is where md5sum,
# sha1sum and sha256sum all put it; the rest of the line is the name of the
# file, and none of that is wanted.
\echo '##''# 100'; {program} {path} 2>/dev/null | ( \read sum rest; \echo "D$sum" ); \echo '##''# 200'
