# Makes a directory on the device, and every directory above it.
\mkdir -p {path} 2>/dev/null && \echo '##''# 000' || \echo '##''# 500'
