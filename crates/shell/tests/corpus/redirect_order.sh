# Redirection (milestone 5's plan 1, programmable shell gate §7.1, §7.2):
# several per command, made left to right; a copy is of the other fd as
# it is at that point; errors only from GNU's programs, never bash's.
echo one > f
echo two >> f
cat f
echo three > f > g
wc -c f g
cat 0< g
wc -l < g
# Both outputs into one file, or the errors where the output was.
cat g missing > both 2>&1; echo $?
cat both
cat g missing 2>&1 > out; echo $?
cat out
cat g missing 2> err >> out; echo $?
cat out err
# Output sent where the errors go, and errors appended.
echo to-err 2> e >&2
echo more 2>> e 1>&2
cat e
# A copy of an fd that changes later is not changed with it.
echo first 2> e3 >&2 2> e2
wc -c e3 e2
# Only redirections: the file is made, or emptied.
> made
wc -c made e
>> made
>e
wc -c made e
