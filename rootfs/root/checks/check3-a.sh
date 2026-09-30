# NUC check 3, part 1 (docs/hardware-test.md): `sh checks/check3-a.sh`,
# then `reboot` and `sh checks/check3-b.sh`. What each command prints goes
# into check3-a.log; in Linux Mint `cargo xtask verify-usb` checks it
# against the `#>` lines below. The `checks` QEMU scenario runs this file too.
#
# Under a command: `#> regex` is one whole line of its output, in order;
# `#> ...` any number of lines; `#nuc>` and `#qemu>` apply only on that
# machine, so a line whose text differs is written twice, `#qemu>` then
# `#nuc>`; `#!> regex` must match no line. A command with no `#>` line
# must print nothing.

# Start afresh, so the check can be run again.
rm -rf /root/notes

uname -a
#> Relay relay 0\.2\.0 x86_64
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d

# Every startup line is [ ok ]; on the NUC, the values of checks 1-3.
dmesg
#> ...
#qemu> \[ ok \] console \d+x\d+ \(\d+x\d+ cells\)
#nuc> \[ ok \] console 1920x1200 \(120x33 cells\)
#> \[ ok \] cpu tables
#> cpu: SMEP on, SMAP on
#qemu> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
#nuc> \[ ok \] boot info: 159\d\d MiB usable in \d+ regions, cmdline ''
#> \[ ok \] memory: \d+ MiB free of \d+ MiB, heap 32 MiB
#> ...
#qemu> \[ ok \] acpi: .*
#nuc> \[ ok \] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
#qemu> \[ ok \] timer: .*
#nuc> \[ ok \] timer: TSC 2496\.000 MHz \(CPUID 0x15\), 1000 Hz tick \(xAPIC\)
#> \[ ok \] rtc: 20\d\d-\d\d-\d\d \d\d:\d\d:\d\d UTC
#> ...
#qemu> \[ ok \] pci: .*
#nuc> \[ ok \] pci: 24 devices on buses 00 01 72, xHCI at 00:0d\.0 00:14\.0
#> ...
#nuc> usb: 00:14\.0 xHCI 1\.20, 16 ports \(12 USB 2, 4 USB 3\), 32-byte contexts, 34 scratchpads
#nuc> ...
#nuc> storage: slot \d+: vendor "Kingston", product "DataTraveler 3\.0", revision "PMAP", removable
#nuc> storage: slot \d+: 30277632 blocks of 512 bytes
#nuc> ...
#nuc> usb: 00:14\.0 port 1: 046d:c534 full-speed, keyboard
#nuc> usb: 00:14\.0 port 3: 046d:c31c low-speed, keyboard
#nuc> usb: 00:14\.0 port 10: 8087:0033 full-speed, not claimed
#nuc> usb: 00:14\.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3\.0, 14\.4 GiB
#qemu> \[ ok \] usb: .*
#nuc> \[ ok \] usb: 2 controllers, 4 devices
#qemu> \[ ok \] keyboard: .*
#nuc> \[ ok \] keyboard: 2 keyboards
#> ...
#nuc> storage: root on 00:14\.0 port 15, the disk with the boot partition [0-9A-F-]{36}
#qemu> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 1
#> ...
#!> \[FAIL\].*

# The programs of /bin: the system archive the loader read from the ESP.
ls -l /bin
#> total \d+
#> ...
#> -rwxr-xr-x 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d t-args
#> ...

# Programs in ring 3 (milestone 2, plan 2): t-args prints its arguments,
# and a fault ends t-fault, not the kernel.
t-args a 'b c' ''
#> \[1\] a
#> \[2\] b c
#> \[3\]\x20
t-fault null-read
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)

# Processes and scheduling (milestone 2, plan 3a): t-spin never makes a
# system call and still ends after its second, the timer taking the CPU
# from it; a hundred children lose no frame; a child killed while its
# parent sleeps; an SSE instruction, a spin with AC set and a program
# setting its own gs base (whatever the firmware left in CR4), killed
# without harm to the kernel.
t-spin 1
#> \d+ iterations
t-spawn 100
#> free frames before: \d+
#> free frames after: \d+
#> frames lost: 0
t-spawn kill
#> t-spin: kill
#> kill 1: EPERM
#> kill 999999: ESRCH
#> pid above 1: true
t-fault sse
#> relay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)
t-fault flags-ac
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault gsbase
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)

# The file operations of the fileops scenario.
mkdir -p /root/notes/old
echo remember me > /root/notes/a
echo and me >> /root/notes/a
cat /root/notes/a
#> remember me
#> and me
ls -l /root/notes
#> total 8
#> -rw-r--r-- 1 root root   19 \w{3} [ \d]\d \d\d:\d\d a
#> drwxr-xr-x 2 root root 4096 \w{3} [ \d]\d \d\d:\d\d old
cp /root/notes/a /root/notes/b
mv /root/notes/b /root/notes/old/c
ls /root/notes /root/notes/old
#> /root/notes:
#> a  old
#> 
#> /root/notes/old:
#> c
rmdir /root/notes/old
#> rmdir: failed to remove '/root/notes/old': Directory not empty
rm -r /root/notes/old
ls /root/notes
#> a
touch /root/notes/t
stat /root/notes/t
#>   File: /root/notes/t
#>   Size: 0 .*regular empty file
#> ...
head -n 1 /root/notes/a
#> remember me
tail -n 1 /root/notes/a
#> and me
wc /root/notes/a
#>  2  4 19 /root/notes/a
cat /root/nope
#> cat: /root/nope: No such file or directory
rm -r /
#> rm: it is dangerous to operate recursively on '/'

# An 8 MiB file (double-indirect blocks), built by doubling as in the
# bigfile scenario: each step writes and syncs up to 4 MiB on the stick.
echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
wc /root/notes/big
#> \s*131072\s+131072\s+8388608 /root/notes/big
cp /root/notes/big /root/notes/copy
wc /root/notes/copy
#> \s*131072\s+131072\s+8388608 /root/notes/copy
rm /root/notes/tmp /root/notes/copy
ls /root/notes
#> a  big  t
df
#> Filesystem +1K-blocks +Used +Available +Use% Mounted on
#qemu> /dev/root +\d+ +\d+ +\d+ +\d+% /
#nuc> /dev/root +20\d{5} +\d+ +\d+ +\d+% /
