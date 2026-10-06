# Whether the device can say a sha1sum of a file.
#
# The program is tried, not asked after: `command -v` answers for what stands
# in the path, which on a device whose utilities are applets of one binary is a
# different question. `Hsha1sum` if it works, `Esha1sum` if it does not.
\echo '##''# 100'; \echo x | \sha1sum >/dev/null 2>&1 && \echo Hsha1sum || \echo Esha1sum; \echo '##''# 200'
