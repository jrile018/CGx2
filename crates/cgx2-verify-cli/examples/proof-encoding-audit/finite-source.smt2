(set-option :produce-models true)
(set-logic QF_UFLIA)
; i64 values are mathematical Ints constrained to the signed 64-bit range.
; Uninterpreted functions, if declared, represent pure total external results.
(declare-const v_78 Int)
(assert (>= v_78 (- 9223372036854775808)))
(assert (<= v_78 9223372036854775807))
; vc:requires_defined
(push 1)
(assert (not (and true true)))
(check-sat)
(pop 1)
; vc:requires_satisfiable
(push 1)
(assert (and (and true true) (>= v_78 0)))
(check-sat)
(pop 1)
; vc:ensures_false
(push 1)
(assert (and (and true true) (>= v_78 0) (and (and (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)) (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807))) (and (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)) (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))) (not (and (= (+ v_78 1) (+ v_78 1)) (= (+ v_78 1) (+ v_78 1))))))
(check-sat)
(pop 1)
; vc:ensures_overflow
(push 1)
(assert (and (and true true) (>= v_78 0) (not (and (and (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)) (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807))) (and (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)) (and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807)))))))
(check-sat)
(pop 1)
