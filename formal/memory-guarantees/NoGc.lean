import Std

/-! An executable reference model for allocation attempts inside a fixed stack
of active CGx2 no_gc guards. Not a semantics or compiler refinement proof.
Costs are mathematical naturals; foreign heaps are outside the model. -/
namespace CGx2.MemoryGuarantees

structure Guard where
  limit : Nat
  spent : Nat
  deriving Repr, DecidableEq

structure State where
  guards : List Guard
  allocated : Nat := 0
  collections : Nat := 0
  deriving Repr, DecidableEq

inductive Request where
  | allocate (bytes : Nat)
  | collect
  deriving Repr, DecidableEq

inductive Outcome where
  | ok (next : State)
  | allocationError (unchanged : State)
  | collectionForbidden (unchanged : State)
  deriving Repr, DecidableEq

def Outcome.state : Outcome → State
  | .ok s | .allocationError s | .collectionForbidden s => s

def Valid (s : State) : Prop :=
  ∀ g ∈ s.guards, g.spent ≤ g.limit

def fits (guards : List Guard) (bytes : Nat) : Bool :=
  guards.all fun g => decide (g.spent + bytes ≤ g.limit)

def charge (guards : List Guard) (bytes : Nat) : List Guard :=
  guards.map fun g => { g with spent := g.spent + bytes }

/-- Check every guard before committing any allocation or budget charge. -/
def attempt (s : State) : Request → Outcome
  | .allocate bytes =>
    if fits s.guards bytes then
      .ok { s with guards := charge s.guards bytes,
                   allocated := s.allocated + bytes }
    else .allocationError s
  | .collect =>
    if s.guards.isEmpty then
      .ok { s with collections := s.collections + 1 }
    else .collectionForbidden s

/-- Sequential attempts, including handled failures. Any fail-fast prefix is
also an input list. Guard entry/exit is outside this block model. -/
def execute (s : State) : List Request → State
  | [] => s
  | request :: rest => execute (attempt s request).state rest

theorem fits_iff (guards : List Guard) (bytes : Nat) :
    fits guards bytes = true ↔
      ∀ g ∈ guards, g.spent + bytes ≤ g.limit := by
  simp [fits, List.all_eq_true]

theorem charge_valid (s : State) (bytes : Nat)
    (h : fits s.guards bytes = true) :
    Valid { s with guards := charge s.guards bytes,
                   allocated := s.allocated + bytes } := by
  intro g hg
  obtain ⟨original, hmem, rfl⟩ := List.mem_map.mp hg
  exact (fits_iff s.guards bytes).mp h original hmem

theorem attempt_valid (s : State) (request : Request)
    (h : Valid s) : Valid (attempt s request).state := by
  cases request with
  | allocate bytes =>
    simp only [attempt]
    split
    · exact charge_valid s bytes (by assumption)
    · exact h
  | collect =>
    simp only [attempt]
    split <;> exact h

theorem attempt_guard_count (s : State) (request : Request) :
    (attempt s request).state.guards.length = s.guards.length := by
  cases request with
  | allocate bytes =>
    simp only [attempt]
    split <;> simp [Outcome.state, charge]
  | collect =>
    simp only [attempt]
    split <;> rfl

theorem attempt_limits (s : State) (request : Request) :
    (attempt s request).state.guards.map Guard.limit =
      s.guards.map Guard.limit := by
  cases request with
  | allocate bytes =>
    simp only [attempt]
    split <;> simp [Outcome.state, charge, List.map_map]
  | collect =>
    simp only [attempt]
    split <;> rfl

theorem active_after_attempt (s : State) (request : Request)
    (h : s.guards ≠ []) : (attempt s request).state.guards ≠ [] := by
  intro hempty
  have hc := attempt_guard_count s request
  rw [hempty] at hc
  exact h (List.length_eq_zero_iff.mp hc.symm)

theorem attempt_no_gc (s : State) (request : Request)
    (h : s.guards ≠ []) :
    (attempt s request).state.collections = s.collections := by
  cases request with
  | allocate bytes =>
    simp only [attempt]
    split <;> rfl
  | collect =>
    simp [attempt, List.isEmpty_iff, h, Outcome.state]

theorem rejected_allocation_unchanged (s : State) (bytes : Nat)
    (g : Guard) (hmem : g ∈ s.guards)
    (hexceeds : ¬ g.spent + bytes ≤ g.limit) :
    attempt s (.allocate bytes) = .allocationError s := by
  have hfalse : fits s.guards bytes = false := by
    cases hf : fits s.guards bytes
    · rfl
    · exact False.elim (hexceeds ((fits_iff s.guards bytes).mp hf g hmem))
  simp [attempt, hfalse]

theorem execute_valid (requests : List Request) (s : State)
    (h : Valid s) : Valid (execute s requests) := by
  induction requests generalizing s with
  | nil => exact h
  | cons request rest ih =>
    exact ih (attempt s request).state (attempt_valid s request h)

theorem execute_limits (requests : List Request) (s : State) :
    (execute s requests).guards.map Guard.limit = s.guards.map Guard.limit := by
  induction requests generalizing s with
  | nil => rfl
  | cons request rest ih =>
    exact (ih (attempt s request).state).trans (attempt_limits s request)

theorem execute_no_gc (requests : List Request) (s : State)
    (h : s.guards ≠ []) : (execute s requests).collections = s.collections := by
  induction requests generalizing s with
  | nil => rfl
  | cons request rest ih =>
    exact (ih (attempt s request).state (active_after_attempt s request h)).trans
      (attempt_no_gc s request h)

/-- A comparison policy deliberately omitting guard checks. This represents
neither Rust nor OCaml nor Python: it is an explicit unconstrained baseline. -/
def unguarded (s : State) : Request → State
  | .allocate bytes =>
    { s with guards := charge s.guards bytes, allocated := s.allocated + bytes }
  | .collect => { s with collections := s.collections + 1 }

def witness : State := { guards := [{ limit := 1, spent := 0 }] }

theorem budget_separation :
    Valid witness ∧
    attempt witness (.allocate 2) = .allocationError witness ∧
    ¬ Valid (unguarded witness (.allocate 2)) := by
  simp [Valid, witness, attempt, unguarded, fits, charge]

theorem gc_separation :
    attempt witness .collect = .collectionForbidden witness ∧
    (unguarded witness .collect).collections = witness.collections + 1 := by
  decide

-- Exact fit, inner exhaustion, outer exhaustion, cumulative charges surviving
-- failure, and collection permitted outside all guards.
example : (attempt witness (.allocate 1)).state.guards =
    [{ limit := 1, spent := 1 }] := by decide
example : attempt { guards := [{ limit := 10, spent := 0 },
                               { limit := 1, spent := 0 }] } (.allocate 2) =
    .allocationError { guards := [{ limit := 10, spent := 0 },
                                 { limit := 1, spent := 0 }] } := by decide
example : attempt { guards := [{ limit := 1, spent := 0 },
                               { limit := 10, spent := 0 }] } (.allocate 2) =
    .allocationError { guards := [{ limit := 1, spent := 0 },
                                 { limit := 10, spent := 0 }] } := by decide
example : (execute witness [.allocate 1, .allocate 1, .collect]).allocated = 1 := by
  decide
example : (attempt { guards := [] } .collect).state.collections = 1 := by decide

#print axioms execute_valid
#print axioms execute_limits
#print axioms execute_no_gc
#print axioms rejected_allocation_unchanged
#print axioms budget_separation
#print axioms gc_separation

end CGx2.MemoryGuarantees
