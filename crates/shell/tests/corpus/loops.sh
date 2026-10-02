# while and until: the body runs while the condition allows; the status
# is the body's last, 0 when the body never runs.
while false; do echo no; done; echo $?
false; until true; do echo no; done; echo $?
A=
while ! echo "$A" | grep xxx; do A=${A}x; echo "pass $A"; done
B=
until echo "$B" | grep yy; do B=${B}y; done; echo $?
C=
while echo "$C" | grep -v zz; do C=${C}z; false; done; echo $?
# The condition's status is $? in the body; a loop in lists, negated.
D=
until echo "$D" | grep d; do echo "status $?"; D=d; done
! while false; do true; done; echo $?
true && while false; do true; done && echo and
# Across lines, nested with if.
E=
while
  ! echo "$E" | grep ee
do
  E=${E}e
  if echo "$E" | grep -v ee; then
    echo "one $E"
  else
    echo "two $E"
  fi
done
F=
until echo "$F" | grep ff; do G=; while ! echo "$G" | grep gg; do G=${G}g; done; F=${F}f; done
