# if, elif and else: the first condition whose status is 0 runs its
# body; the status is the body's, 0 when no branch runs.
if true; then echo then1; fi
if false; then echo no; fi; echo $?
if false; then echo no; else echo else1; fi
if false; then echo no; elif true; then echo elif1; else echo no; fi
if false; then echo no; elif false; then echo no; else echo else2; fi
if false; then echo no; elif false; then echo no; fi; echo $?
if true; then false; fi; echo $?
if false; then true; else seq 0; false; fi; echo $?
false; if false; then true; fi; echo $?
# A condition is a list; its status is $? in the body.
if false; true; then echo list1; fi
if true && false; then echo no; else echo $?; fi
if false || true; then echo or1; fi
if seq 3 | grep 2; then echo pipe1; fi
if ! false; then echo bang1; fi
! if false; then true; fi; echo $?
# In lists and and-or lists, and nested.
echo before; if true; then echo inside; fi; echo after
true && if true; then echo and1; fi || echo no
if false; then echo no; fi || echo or2
if true; then if false; then echo no; else echo nested1; fi; fi
if if true; then false; fi; then echo no; else echo nested2; fi
if true; then if true; then echo nested3; fi fi
# Across lines, blank and comment lines among them.
if true
then
  echo lines1

  # a comment
elif false
then
  echo no
else
  echo no
fi
if
  false
then echo no; else echo lines2; fi
# Its words are keywords only where a command name stands.
echo if then elif else fi
A=fi; echo $A
if true; then echo "then" 'fi'; fi
