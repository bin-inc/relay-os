# NUC check 4 (docs/hardware-test.md; milestone 2, plan 4b): after
# check3-b.sh, `sh checks/check4.sh`. Every command here is a program in
# ring 3, run by /bin/sh, which init started as process 2. Its transcript
# is check4.log; the `#>` lines are explained in check3-a.sh, and
# `#same> NAME regex` is a line whose group must capture what it captured
# the first time.

# Start afresh, so the check can be run again.
rm -f /root/check4.list /root/check4-a.sh /root/check4-a.log /root/check4-b.sh /root/check4-b.log

# Filling the process table makes the page tables of every kernel-stack
# slot, which stay: memory in use compares after it. Init, this shell and
# the script's leave room for 60 children, and init collects them as they
# end, a second later.
t-spawn fill
#> filled the table with 60 children
t-spin 1
#> \d+ iterations
t-spawn 1000
#> free frames before: \d+
#> free frames after: \d+
#> frames lost: 0
free
#> \s+total\s+used\s+free
#same> used Mem:\s+\d+\s+(\d+)\s+\d+
#> Heap:\s+\d+\s+\d+\s+\d+

# Every kind of fault kills the program, not the kernel, with its message
# and the kernel log's line (milestone 2, plans 2 and 3a).
t-fault null-read
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)
t-fault null-write
#> relay-sh: t-fault: killed \(page fault at 0x0, write, ip 0x4[0-9a-f]+\)
t-fault write-code
#> relay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, write, ip 0x4[0-9a-f]+\)
t-fault exec-data
#> relay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, execute, ip 0x4[0-9a-f]+\)
t-fault ud
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault div0
#> relay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)
t-fault sse
#> relay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)
t-fault gsbase
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault stack
#> relay-sh: t-fault: killed \(stack overflow at 0x7fffffeff[0-9a-f]{3}, ip 0x4[0-9a-f]+\)
t-fault kernel-read
#> relay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)
t-fault flags-exit
t-fault flags-ud
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault flags-tf
#> relay-sh: t-fault: killed \(CPU exception 1, ip 0x4[0-9a-f]+\)
t-fault flags-ac
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)

# Processes: one killed before it ran, and an orphan (a t-spin of a
# second, whose line comes during the next one), which init collects when
# it ends; a program built for another ABI is refused (plans 3a and 4b).
t-spawn kill-new
#> t-args: kill
t-spawn orphan
t-spin 1
#> \d+ iterations
#> \d+ iterations
t-args a 'b c' ''
#> \[1\] a
#> \[2\] b c
#> \[3\]\x20
t-abi
#> relay-sh: t-abi: Exec format error

# Under /bin/sh a program's standard output is the file itself: lines,
# not columns, and never its own input (plan 4b).
ls /bin > /root/check4.list
cat /root/check4.list
#> cat
#> clear
#> cp
#> ...
#> wc
cat /root/check4.list >> /root/check4.list
#> cat: /root/check4.list: input file is output file

# A script that runs another: both transcripts get the inner one's lines,
# this one included (plan 4a).
echo 'echo one' > /root/check4-a.sh
echo 'sh /root/check4-b.sh' >> /root/check4-a.sh
echo 'echo two' > /root/check4-b.sh
sh /root/check4-a.sh
#> \+ echo one
#> one
#> \+ sh /root/check4-b.sh
#> \+ echo two
#> two
cat /root/check4-b.log
#> \+ echo two
#> two

# The same memory in use as before all of it.
free
#> \s+total\s+used\s+free
#same> used Mem:\s+\d+\s+(\d+)\s+\d+
#> Heap:\s+\d+\s+\d+\s+\d+
