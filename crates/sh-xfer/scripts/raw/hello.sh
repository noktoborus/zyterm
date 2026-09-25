# What the device needs to carry a body as the bytes themselves.
#
# Every tool is tried, not asked after: `command -v` answers for what stands in
# the path, which on a device whose utilities are applets of one binary is a
# different question. A tool that is there answers H, one that is not answers
# E, and an E ends the conversation before anything travels.
#
# `stty -g` is the right probe twice over: it says whether `stty` works *on
# this line*, which on a pipe it does not, however installed it is.
\echo '##''# 100';
\echo xy | \tail -c +2 >/dev/null 2>&1 && \echo Htail || \echo Etail;
\echo x | \head -c 1 >/dev/null 2>&1 && \echo Hhead || \echo Ehead;
\echo x | \wc -c >/dev/null 2>&1 && \echo Hwc || \echo Ewc;
\stty -g >/dev/null 2>&1 && \echo Hstty || \echo Estty;
\echo '##''# 200'
