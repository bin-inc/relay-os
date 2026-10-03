# NUC check 6 (docs/hardware-test.md; milestone 4, plan 4): after
# check5.sh, `sh checks/check6.sh`. Lists, compound commands and `test`,
# run by /bin/sh. Its transcript is check6.log; the `#>` lines are
# explained in check3-a.sh. A construct written across lines is traced
# line by line before it runs, so its output's `#>` lines follow its last
# line, and each of its lines counts as a command. Only the time differs
# between the NUC and QEMU.

# Start afresh, so the check can be run again.
rm -f /root/check6-a.sh /root/check6-a.log /root/check6-go /root/check6-stop /root/check6-old /root/check6-new

# if, elif and else, across lines and on one.
for n in 1 2 3
do
  if [ $n = 1 ]
  then
    echo one
  elif [ $n = 2 ]; then
    echo two
  else
    echo other $n
  fi
done
#> one
#> two
#> other 3
if false; then echo no; fi
echo $?
#> 0

# while and until, each ended by a file it makes or removes.
touch /root/check6-go
while [ -f /root/check6-go ]; do rm /root/check6-go; echo while once; done
#> while once
until [ -f /root/check6-stop ]; do touch /root/check6-stop; echo until once; done
#> until once
ls /root/check6-go /root/check6-stop
#> ls: cannot access '/root/check6-go': No such file or directory
#> /root/check6-stop

# for over words, and over "$@" in a script given arguments: another
# script, since one run inside itself would empty its own transcript.
for w in a 'b c' "$E" d
do echo "[$w]"
done
#> \[a\]
#> \[b c\]
#> \[\]
#> \[d\]
echo 'for a in "$@"; do echo "in [$a]"; done' > /root/check6-a.sh
echo 'for a do echo "do [$a]"; done' >> /root/check6-a.sh
sh /root/check6-a.sh one '' 'two  words'
#> \+ for a in "\$@"; do echo "in \[\$a\]"; done
#> in \[one\]
#> in \[\]
#> in \[two  words\]
#> \+ for a do echo "do \[\$a\]"; done
#> do \[one\]
#> do \[\]
#> do \[two  words\]

# The statuses of &&, || and !.
true && echo and
#> and
false || echo or
#> or
false && echo no; echo $?
#> 1
! true; echo $?
#> 1
! false; echo $?
#> 0
true && false || echo both
#> both

# test and [ on the stick's files.
[ -d /bin ] && [ -f /bin/ls ] && [ -x /bin/sh ] && echo bin
#> bin
test -s /root/checks/check6.sh && [ -w /root ] && echo root
#> root
[ -w /bin/ls ]; echo $?
#> 1
[ -e /root/nothing ] || [ ! -e /root/nothing ] && echo nothing
#> nothing
touch /root/check6-old
sleep 1
touch /root/check6-new
[ /root/check6-new -nt /root/check6-old ] && [ /root/check6-old -ot /root/check6-new ] && echo newer
#> newer
[ 3 -lt 10 ] && [ abc = abc ] && [ abc != abd ] && echo compare
#> compare
[ 1 -eq x ]; echo $?
#> \[: invalid integer 'x'
#> 2

# 100 programs, each followed by a sync, timed: the results log records
# the two times (spec §13).
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d
for a in 0 1 2 3 4 5 6 7 8 9
do
  for b in 0 1 2 3 4 5 6 7 8 9; do true; done
done
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d
