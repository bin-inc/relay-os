# !: a pipeline's status negated, each ! turning it again.
! true; echo $?
! false; echo $?
! ! true; echo $?
! ! ! true; echo $?
! true | false; echo $?
! false && echo negated-and
! true || echo negated-or
! ; echo $?
