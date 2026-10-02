# for: the body once for each word, the variable set first and kept.
for x in a b c; do echo "$x"; done
for x in a b; do echo "$x"; done; echo "last $x"
for x in; do echo no; done; echo $?
for x in a 'b c' "" $E; do echo "[$x]"; done
for x in if then do done; do echo "$x"; done
for done in a; do echo "$done"; done
for x in a b; do false; done; echo $?
# Without words, or with "$@", over the arguments: none here.
for x; do echo no; done
for x do echo no; done
for x in "$@" a; do echo "[$x]"; done
# Across lines, and nested.
for x in a b
do
  echo "$x"
done
for x
in c d
do echo "$x"; done
for x in a b; do for y in 1 2; do echo "$x$y"; done; done
for x in a b c; do if echo "$x" | grep b; then echo found; fi; done
for x in a b; do H=; until echo "$H" | grep "$x"; do H=$x; done; done
