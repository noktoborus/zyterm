# One chunk of a file, as base64, which any line carries whatever its settings.
#
# `tail -c +{offset}` starts at the byte the chunk belongs at — the offset is
# counted from one, which is what `tail` counts in — and `head -c` stops after
# the bytes of the chunk. Both read the file in blocks of their own choosing,
# so a chunk costs the device the bytes it carries and not a system call for
# every one of them, which `dd bs=1` with an offset does.
\echo '##''# 100';
\tail -c +{offset} {path} 2>/dev/null | \head -c {count} | \base64;
\echo '##''# 200'
