# What `scp -vv` says

What `scp` of OpenSSH 9.2 wrote to stderr in five transfers, against an
`sshd` of the same version on the loopback, read back by `tests/scp.rs` through
`scripts/lib/scp.lua`. The two files are 300 KiB and 1 MiB of random bytes,
and the speed is held at 4000 Kbit/s (`-l 4000`) so the windows of the channel
move many times in each.

| file | arguments | what it is |
| --- | --- | --- |
| `send-legacy.txt` | `-O -vv one.bin two.bin root@127.0.0.1:back/` | the old protocol, both files over one connection |
| `send-sftp.txt` | `-vv one.bin two.bin root@127.0.0.1:back/` | SFTP, the default since OpenSSH 9.0 |
| `receive-legacy.txt` | `-O -vv -T root@127.0.0.1:there/one.bin root@127.0.0.1:there/two.bin back/` | the old protocol the other way, a connection for each path |
| `receive-sftp.txt` | `-vv root@127.0.0.1:there/one.bin root@127.0.0.1:there/two.bin back/` | SFTP the other way |
| `send-legacy-v.txt` | `-O -v one.bin root@127.0.0.1:back/` | one `-v`, which says nothing of the bytes |

Every run also had `-i KEY -P 2222 -o StrictHostKeyChecking=no -o
UserKnownHostsFile=/dev/null -o BatchMode=yes -l 4000`. The paths of the
machine they were recorded on are `/home/user/work` and `/home/user/.ssh`, and
the fingerprints of the keys are `SHA256:<key>`; nothing else was changed.

To record them again, run an `sshd` that takes a key, and the commands above
with stderr to the file:

```sh
scp -i KEY -P 2222 … -O -vv one.bin two.bin root@127.0.0.1:back/ 2> send-legacy.txt
```
