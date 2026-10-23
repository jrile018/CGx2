(set-option :produce-models true)
(set-logic QF_UFLIA)
; i64 values are mathematical Ints constrained to the signed 64-bit range.
; Uninterpreted functions, if declared, represent pure total external results.
(declare-const v_666c6167 Bool)
(declare-const v_78 Int)
(assert (>= v_78 (- 9223372036854775808)))
(assert (<= v_78 9223372036854775807))
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
(assert (and true true (and (and (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true)) (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true))) (and (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true)) true)) (not (and (= (ite v_666c6167 (ite (< v_78 0) 0 v_78) 1) (ite v_666c6167 (ite (< v_78 0) 0 v_78) 1)) (>= (ite v_666c6167 (ite (< v_78 0) 0 v_78) 1) 0)))))
(check-sat)
(pop 1)
; vc:ensures_overflow
(push 1)
(assert (and true true (not (and (and (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true)) (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true))) (and (and true (ite v_666c6167 (and (and true true) (ite (< v_78 0) true true)) true)) true)))))
(check-sat)
(pop 1)
