# NUC check 5 (docs/hardware-test.md; milestone 3, plan 4): after
# check4.sh, `sh checks/check5.sh`. Pipes, a background job and `kill`,
# script arguments and variables, run by /bin/sh. Its transcript is
# check5.log; the `#>` lines are explained in check3-a.sh. Nothing here
# differs between the NUC and QEMU.

# Start afresh, so the check can be run again.
rm -f /root/check5-a.sh /root/check5-a.log /root/check5-b.sh /root/check5-b.log

# Pipes (plan 1): 6.9 MB through a 16 KiB pipe, a reader that ends first
# (seq then ends quietly), three commands, the last command's status,
# and a shell reading its commands from a pipe.
seq 1000000 | wc -c
#> 6888896
seq 1000000 | head -n 1
#> 1
seq 1000 | grep 7 | wc -l
#> 271
seq 5 | grep -c 9
#> 0
echo $?
#> 1
echo 'echo piped $#' | sh
#> piped 0

# A background job (plan 2). A script prints no `[1] <pid>` and no Done
# line; `kill` ends the job, `wait` collects it and gives its status.
t-spin &
jobs
#> \[1\]\+  Running                 t-spin &
ps | grep -c t-spin
#> 1
kill %1
wait %1
echo $?
#> 137
jobs
ps | grep -c t-spin
#> 0

# Script arguments and variables (plan 3): a script given three
# arguments, one empty and one with two blanks, passes them on to
# another with one more. A value is never split into words (bash would
# split `$A`), an unquoted empty one is no word, and `cd "$9"` without a
# ninth argument stays where it is (not home, where it started).
echo 'echo "$0: $# arguments"' > /root/check5-a.sh
echo 't-args "$@"' >> /root/check5-a.sh
echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
echo 'cd /root/checks' >> /root/check5-b.sh
echo 'cd "$9"' >> /root/check5-b.sh
echo 'echo "cd: $?"' >> /root/check5-b.sh
echo 'pwd' >> /root/check5-b.sh
sh /root/check5-a.sh one '' 'two  words'
#> \+ echo "\$0: \$# arguments"
#> /root/check5-a\.sh: 3 arguments
#> \+ t-args "\$@"
#> \[1\] one
#> \[2\]\x20
#> \[3\] two  words
#> \+ sh /root/check5-b\.sh "\$@" last
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd /root/checks
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root/checks
cat /root/check5-b.log
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd /root/checks
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root/checks
A='a  b'
t-args $A "$A" '$A' ${A}c $E "$E"
#> \[1\] a  b
#> \[2\] a  b
#> \[3\] \$A
#> \[4\] a  bc
#> \[5\]\x20
false
echo $?
#> 1
