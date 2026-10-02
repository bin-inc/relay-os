# test and [ as conditions (plan 3): bash runs its built-in test, which
# answers these as GNU's program does, and neither prints a message
# (bash's cannot read integers past 64 bits, which GNU's can).
X=abc
if [ "$X" = abc ]; then echo equal; fi
if test -n "$X" && [ -z "" ]; then echo both; fi
[ "$X" != abd ] && echo differ
[ 1 -lt 2 ] && echo less
[ 10 -gt 9 ] || echo not-run
[ -0 -eq 0 ] && [ +7 -eq 007 ] && echo signs
[ ! -e /nonexistent ] && echo absent
[ -d / ] && test -d /etc && echo dirs
[ / -ef / ] && echo same
[ a = b -o c = c ] && echo or
[ a = a -a ! b = c ] && echo and
[ \( a = a \) -a \( '' -o x \) ] && echo parens
test && echo no-arguments || echo false
test '' || echo empty
test x && echo string
[ ! ] && echo bang
[ -n ] && echo dash-n
[ é = é ] && echo accent
# In loops: a counter kept as a string of x's, ended by test.
N=
while [ "$N" != xxx ]; do N=${N}x; echo "pass $N"; done
until test "$N" = ""; do N=; echo cleared; done
for n in 1 2 3; do
  if [ "$n" -eq 2 ]; then
    echo two
  elif [ "$n" -lt 2 ]; then
    echo small
  else
    echo big
  fi
done
# The statuses: 0 for true, 1 for false.
[ 1 -eq 2 ]; echo $?
[ x ]; echo $?
! [ x ]; echo $?
