# A command goes on after |, && or ||, past blank and comment lines.
true &&
  echo joined
false ||

# a comment between
echo after-blank
seq 3 |
cat
false ||
  false ||
  echo third-line; echo $?
