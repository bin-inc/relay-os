# Redirection of compound commands and pipelines (milestone 5's plan 1,
# programmable shell gate §7.3, §7.4): made once, for everything inside;
# every command inside shares the file's offset.
seq 5 > lines
for n in 1 2 3; do head -n 1; done < lines
if grep -q 3; then echo found; fi < lines
for n in a b; do echo $n; cat lines | wc -l; done > out
cat out
while false; do echo never; done > none
wc -c none
for n in x y; do echo $n; cat missing; done > both 2>&1
cat both
for n in 1; do cat missing; done 2> err; echo $?
cat err
if true; then echo kept; cat missing; fi 2>&1 > kept
cat kept
# Each command of a pipeline over its pipes.
cat lines missing 2>&1 | wc -l
cat missing 2> err | wc -l
cat err
cat lines | head -n 2 2> err | wc -l
# A redirection inside a construct goes over the construct's.
for n in 1; do echo in > inner; echo outer; done > outside
cat inner outside
# Loops that read on through one file.
seq 4 > four
for n in a b; do head -n 2 | cat; done < four
