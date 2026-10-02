# && and ||: equal precedence, left to right; a pipeline that does not run
# leaves $? alone.
true && echo and1
false && echo no
false || echo or1
true || echo no
false || echo or2 && echo and2
true && false || echo or3
true || false && echo and3
false && echo no; echo $?
true && false; echo $?
false || false; echo $?
seq 2 | false || echo after-pipeline
A=x && echo $A
