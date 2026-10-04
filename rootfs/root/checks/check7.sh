# NUC check 7 (docs/hardware-test.md; milestone 5, plan 4): after
# check6.sh, `cd checks` and `sh check7.sh`, so that it starts in
# /root/checks. Redirection, /dev/null, the environment and cd, run by
# /bin/sh. Its transcript is check7.log; the `#>` lines are explained in
# check3-a.sh. Nothing here differs between the NUC and QEMU.

# Run from any directory (programmable shell gate §9.2): where it was
# started is OLDPWD below.
cd /root

# Start afresh, so the check can be run again.
rm -f /root/check7-a.sh /root/check7-a.log /root/check7-out /root/check7-err

# /dev/null, and every standard stream redirected: both orders of
# 2>&1, into a pipe, a loop's output, appending.
[ -c /dev/null ] && echo null is a character device
#> null is a character device
echo lost > /dev/null
cat /dev/null | wc -c
#> 0
ls /nope 2> /dev/null; echo $?
#> 2
wc -c < /dev/null
#> 0
ls /root/checks/check7.sh /nope > /root/check7-out 2> /root/check7-err
cat /root/check7-out /root/check7-err
#> /root/checks/check7\.sh
#> ls: cannot access '/nope': No such file or directory
ls /nope /root/checks/check7.sh > /root/check7-out 2>&1
cat /root/check7-out
#> ls: cannot access '/nope': No such file or directory
#> /root/checks/check7\.sh
ls /nope /root/checks/check7.sh 2>&1 > /root/check7-out
#> ls: cannot access '/nope': No such file or directory
cat /root/check7-out
#> /root/checks/check7\.sh
ls /nope /root/checks/check7.sh 2>&1 | wc -l
#> 2
for w in a b c; do echo $w; done > /root/check7-out
wc -l < /root/check7-out
#> 3
echo more >> /root/check7-out
cd /nope 2>> /root/check7-out
tail -n 2 /root/check7-out
#> more
#> relay-sh: cd: /nope: No such file or directory

# export, env -i and -u, an assignment before a command, a nested sh
# that sees what is exported, and unset.
export A=1
env | grep ^A=
#> A=1
B=2 env | grep ^B=
#> B=2
echo "[$B]"
#> \[\]
env -i C=3 D=4 env
#> C=3
#> D=4
env -u HOME env | grep -c ^HOME=
#> 0
echo 'echo "nested: [$A] [$B]"' > /root/check7-a.sh
B=5 sh /root/check7-a.sh
#> \+ echo "nested: \[\$A\] \[\$B\]"
#> nested: \[1\] \[5\]
unset A
env | grep -c ^A=
#> 0
sh /root/check7-a.sh
#> \+ echo "nested: \[\$A\] \[\$B\]"
#> nested: \[\] \[\]

# cd -, PWD and OLDPWD: the script started in /root/checks.
echo "$PWD $OLDPWD"
#> /root /root/checks
cd /bin
echo "$PWD $OLDPWD"
#> /bin /root
cd -
#> /root
pwd
#> /root
cd ..
pwd
#> /
cd
echo "$PWD $OLDPWD"
#> /root /
