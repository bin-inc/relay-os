# NUC check 3, part 2 (docs/hardware-test.md): after check3-a.sh and a
# `reboot`, `sh checks/check3-b.sh`, then checks 4 and 5. Its transcript is
# check3-b.log; the `#>` lines are explained in check3-a.sh.

# The files written before the restart are still there.
cat /root/notes/a
#> remember me
#> and me
ls /root/notes
#> a  big  t
wc /root/notes/big
#> \s*131072\s+131072\s+8388608 /root/notes/big
tail -n 1 /root/notes/big
#> 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde

# The machine started again as before.
dmesg
#> ...
#qemu> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> ...
#!> \[FAIL\].*
