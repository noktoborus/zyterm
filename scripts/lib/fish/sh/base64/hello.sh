# What the device needs to carry a body as base64.
#
# Every tool is tried, not asked after: `command -v` answers for what stands in
# the path, which on a device whose utilities are applets of one binary is a
# different question. A tool that is there answers H, one that is not answers
# E, and an E ends the conversation before anything travels.
#
# Nothing of the line is touched here. The command that carries a body is the
# only one entitled to change it, and only for as long as it runs.
\echo '##''# 100';
\echo x | \base64 >/dev/null 2>&1 && \echo Hbase64 || \echo Ebase64;
\echo xy | \tail -c +2 >/dev/null 2>&1 && \echo Htail || \echo Etail;
\echo x | \head -c 1 >/dev/null 2>&1 && \echo Hhead || \echo Ehead;
\echo x | \wc -c >/dev/null 2>&1 && \echo Hwc || \echo Ewc;
\echo '##''# 200'
