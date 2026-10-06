# Whether the device can say a md5sum of a file.
#
# The program is tried, not asked after: `command -v` answers for what stands
# in the path, which on a device whose utilities are applets of one binary is a
# different question. `Hmd5sum` if it works, `Emd5sum` if it does not.
\echo '##''# 100'; \echo x | \md5sum >/dev/null 2>&1 && \echo Hmd5sum || \echo Emd5sum; \echo '##''# 200'
