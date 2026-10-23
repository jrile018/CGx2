(set-option :produce-models true)
(set-logic QF_UFLIA)
; i64 values are mathematical Ints constrained to the signed 64-bit range.
; Uninterpreted functions, if declared, represent pure total external results.
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
(assert (and true true (and (and (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))) (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807))))) (and (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))) true)) (not (and (= (ite true v_78 (+ v_78 1)) (ite true v_78 (+ v_78 1))) (= (ite true v_78 (+ v_78 1)) v_78)))))
(check-sat)
(pop 1)
; vc:ensures_overflow
(push 1)
(assert (and true true (not (and (and (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))) (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807))))) (and (and true (ite true true (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))) true)))))
(check-sat)
(pop 1)
