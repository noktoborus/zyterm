# The path a directory really is, every symbolic link resolved.
#
# The change of directory happens in a subshell. The shell of the device must
# not be moved: every relative path sent after this would mean something else.
\echo '##''# 100'; ( \cd {path} 2>/dev/null && \pwd -P ); \echo '##''# 200'
