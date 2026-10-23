(set-option :produce-models true)
(set-logic QF_UFLIA)
; i64 values are mathematical Ints constrained to the signed 64-bit range.
; Uninterpreted functions, if declared, represent pure total external results.
; vc:requires_defined
(push 1)
(assert (not true))
(check-sat)
(pop 1)
; vc:requires_satisfiable
(push 1)
(assert (and true true))
(check-sat)
(pop 1)
; vc:ensures_false
(push 1)
(assert (and true true true (not false)))
(check-sat)
(pop 1)
; vc:ensures_overflow
(push 1)
(assert (and true true (not true)))
(check-sat)
(pop 1)
