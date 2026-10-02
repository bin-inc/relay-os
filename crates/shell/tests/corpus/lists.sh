# Lists: ; runs each item in turn, and each reads the $? of the one before.
echo a; echo b
false; echo $?
true; echo $?
echo c;
echo d;echo e;
A=1; B=$A; echo $A $B
seq 3 | cat; echo $?
