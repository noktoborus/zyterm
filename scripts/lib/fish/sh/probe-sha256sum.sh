# Whether the device can say a sha256sum of a file.
#
# The program is tried, not asked after: `command -v` answers for what stands
# in the path, which on a device whose utilities are applets of one binary is a
# different question. `Hsha256sum` if it works, `Esha256sum` if it does not.
\echo '##''# 100'; \echo x | \sha256sum >/dev/null 2>&1 && \echo Hsha256sum || \echo Esha256sum; \echo '##''# 200'
