import BudgetKernel
open Aeneas Aeneas.Std Aeneas.Std.Result Aeneas.Std.ControlFlow
open budget_kernel Aeneas.Std.WP
namespace SourceBudgetProof

-- Machine-sized subtraction in the actual generated branch has the expected Nat value.
theorem saturating_sub_value (limit spent : Usize) :
    (core.num.Usize.saturating_sub limit spent).val = limit.val - spent.val := by
  simp only [core.num.Usize.saturating_sub, UScalar.saturating_sub, UScalar.val,
    Nat.zero_max, BitVec.toNat_ofNat]
  exact Nat.mod_eq_of_lt (Nat.lt_of_le_of_lt (Nat.sub_le _ _) limit.bv.isLt)

theorem admission_equivalence (b : budget.Budget) (bytes : Usize)
    (reachable : b.spent.val ≤ b.limit.val) :
    (¬ bytes > core.num.Usize.saturating_sub b.limit b.spent) ↔
      b.spent.val + bytes.val ≤ b.limit.val := by
  change (¬ (core.num.Usize.saturating_sub b.limit b.spent).val < bytes.val) ↔ _
  rw [saturating_sub_value]
  omega

-- Conditional on the library iterator returning this same reachable guard,
-- the actual extracted body continues precisely when addition fits the budget.
theorem generated_admission (b : budget.Budget) (bytes : Usize)
    (iter iter1 : core.slice.iter.Iter budget.Budget)
    (next : core.slice.iter.IteratorSliceIter.next iter = .ok (some b, iter1))
    (reachable : b.spent.val ≤ b.limit.val)
    (admitted : b.spent.val + bytes.val ≤ b.limit.val) :
    budget.Session.check_budgets_loop.body bytes iter = .ok (.cont iter1) := by
  simp [budget.Session.check_budgets_loop.body, next, Aeneas.Std.lift]
  rw [saturating_sub_value]
  omega

-- No overflow in the identical guard/bytes addition used by the generated charge body.
theorem admitted_addition (b : budget.Budget) (bytes : Usize)
    (reachable : b.spent.val ≤ b.limit.val)
    (admitted : ¬ bytes > core.num.Usize.saturating_sub b.limit b.spent) :
    b.spent + bytes ⦃ sum => sum.val = b.spent.val + bytes.val ∧ sum.val ≤ b.limit.val ⦄ := by
  have hbound := (admission_equivalence b bytes reachable).mp admitted
  have hmax : b.spent.val + bytes.val ≤ UScalar.max .Usize := by scalar_tac
  have hadd := @UScalar.add_equiv UScalarTy.Usize b.spent bytes
  split at hadd
  · rename_i sum heq
    simp at heq
    obtain ⟨_, hval, _⟩ := hadd
    simp [heq, spec_ok, hval, hbound]
  · exfalso
    simp [UScalar.inBounds] at hadd
    scalar_tac
  · exact hadd.elim

-- numBits is opaque in Lean. Record the executable value below and expose
-- the target correspondence assumption rather than introducing an axiom.
theorem target_max (width : System.Platform.numBits = 64) :
    UScalar.max .Usize = 18446744073709551615 := by
  simp [UScalar.max, UScalarTy.numBits, width]
#eval System.Platform.numBits

-- Iterator observations are explicit assumptions. No Vec axiom is used here.
theorem generated_charge_no_overflow
    (b : budget.Budget) (bytes : Usize)
    (iter iter1 : core.slice.iter.IterMut budget.Budget)
    (back : core.slice.iter.IterMut budget.Budget → core.slice.iter.IterMut budget.Budget)
    (nextBack : core.slice.iter.IterMut budget.Budget → Option budget.Budget →
      core.slice.iter.IterMut budget.Budget)
    (next : core.slice.iter.IteratorIterMut.next iter = .ok (some b, iter1, nextBack))
    (reachable : b.spent.val ≤ b.limit.val)
    (admitted : ¬ bytes > core.num.Usize.saturating_sub b.limit b.spent) :
    budget.Session.charge_budgets_loop.body bytes iter back ⦃ _ => True ⦄ := by
  unfold budget.Session.charge_budgets_loop.body
  rw [next]
  simp only [bind_ok]
  apply spec_bind (admitted_addition b bytes reachable admitted)
  intro sum _
  simp [spec_ok]

#print axioms saturating_sub_value
#print axioms admission_equivalence
#print axioms generated_admission
#print axioms admitted_addition
#print axioms target_max
#print axioms generated_charge_no_overflow
end SourceBudgetProof
