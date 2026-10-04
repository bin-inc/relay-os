# The shell's environment (milestone 5's plan 3, programmable shell gate
# §8.5, §9.1): export, unset, assignments before a command, cd and cd -,
# as bash 5.2 runs them. The directory differs, so only what follows
# from it is printed.
A=1
export A B
B=2
export C=3 D
unset A
echo "[$A][$B][$C][$D]"
X=1 true
echo "[$X]"
C=4 true
echo "$C"
E=5 $UNSET
echo "$E"
start=$PWD
mkdir d
cd d
test "$OLDPWD" = "$start" && echo oldpwd
test "$PWD" = "$start/d" && echo pwd
cd - > ../out
test "$PWD" = "$start" && echo back
wc -l < out
cd ""
test "$OLDPWD" = "$start" && echo oldpwd-again
test "$PWD" = "$start" && echo stays
OLDPWD=
cd - > out2
wc -c < out2
cd d/..
test "$PWD" = "$start" && echo canonical
