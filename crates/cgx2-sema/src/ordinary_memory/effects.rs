//! Closed-model resource effects, inferred for every concrete function including
//! private/unused bodies and recursive SCCs. No caller summary is trusted.
use super::{MemoryEffectsV2, MemoryOpV1, RuntimeProgram};
use cgx2_runtime::default_managed::model::{Op, Statement};
use std::collections::{BTreeSet, VecDeque};

impl MemoryEffectsV2 {
    fn include(&mut self, other: &Self) -> bool {
        let old = self.clone();
        self.may_allocate_managed |= other.may_allocate_managed;
        self.may_auto_collect_at_allocation |= other.may_auto_collect_at_allocation;
        self.may_explicitly_collect |= other.may_explicitly_collect;
        self.may_allocate_root_bookkeeping |= other.may_allocate_root_bookkeeping;
        self.may_enter_region |= other.may_enter_region;
        self.may_create_region |= other.may_create_region;
        self.may_enter_guard |= other.may_enter_guard;
        self.may_allocate_owned |= other.may_allocate_owned;
        self.may_transfer_graph |= other.may_transfer_graph;
        self.may_enter_session |= other.may_enter_session;
        *self != old
    }
}

pub(super) fn infer(memory: &RuntimeProgram) -> Vec<MemoryEffectsV2> {
    let p = &memory.program;
    let mut effects = Vec::with_capacity(p.functions.len());
    let mut callers = vec![BTreeSet::new(); p.functions.len()];
    for (index, f) in p.functions.iter().enumerate() {
        let mut e = MemoryEffectsV2 {
            may_allocate_managed: f.effects.may_allocate_managed,
            may_auto_collect_at_allocation: f.effects.may_auto_collect_at_allocation,
            may_explicitly_collect: f.effects.may_explicitly_collect,
            may_allocate_root_bookkeeping: f.effects.may_allocate_root_bookkeeping,
            ..Default::default()
        };
        let mut expressions = f
            .requires
            .iter()
            .chain(&f.ensures)
            .copied()
            .collect::<Vec<_>>();
        let mut statements = f.body.iter().collect::<Vec<_>>();
        while let Some(s) = statements.pop() {
            match s {
                Statement::SLet(_, id) | Statement::SExpr(id) => expressions.push(*id),
                // The middle assignment field is an operation tag, not an ID.
                Statement::SAssign(_, _, id) => expressions.push(*id),
                Statement::SReturn(id) => expressions.extend(id),
                Statement::SIf(id, yes, no) => {
                    expressions.push(*id);
                    statements.extend(yes.iter().chain(no));
                }
                Statement::SMatch(id, arms) => {
                    expressions.push(*id);
                    statements.extend(arms.iter().flat_map(|a| &a.body));
                }
                Statement::SRange(_, a, b, body) => {
                    expressions.extend([*a, *b]);
                    statements.extend(body);
                }
                Statement::SFor(_, id, body) => {
                    expressions.push(*id);
                    statements.extend(body);
                }
                Statement::SGuard(_, _, body) => {
                    // Legacy guards store a literal byte budget, not an ID.
                    e.may_enter_guard = true;
                    e.may_allocate_root_bookkeeping = true;
                    statements.extend(body);
                }
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(id) = expressions.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(op) = memory.operations.get(&id) {
                match op {
                    MemoryOpV1::RegionEntry { .. } => {
                        e.may_enter_region = true;
                        e.may_create_region = true;
                        e.may_allocate_root_bookkeeping = true;
                    }
                    MemoryOpV1::GuardEntry { .. } => {
                        e.may_enter_guard = true;
                        e.may_allocate_root_bookkeeping = true;
                    }
                    MemoryOpV1::SessionEntry => e.may_enter_session = true,
                    MemoryOpV1::CopyTo { .. } => {
                        e.may_transfer_graph = true;
                        e.may_allocate_managed = true;
                        e.may_auto_collect_at_allocation = true;
                        e.may_allocate_root_bookkeeping = true;
                    }
                    MemoryOpV1::Collect { .. } => e.may_explicitly_collect = true,
                    MemoryOpV1::TryIn { .. } | MemoryOpV1::DefaultRegion => {}
                }
            }
            match &p.expressions[id].op {
                Op::ECall(callee, args) => {
                    callers[*callee].insert(index);
                    expressions.extend(args);
                }
                Op::EField(a, _) | Op::ETry(a) | Op::ERef(a) | Op::EGet(a) => expressions.push(*a),
                Op::EBinary(a, _, b) | Op::ESet(a, b) => expressions.extend([*a, *b]),
                Op::ERecord(fields) => expressions.extend(fields.iter().map(|(_, id)| id)),
                Op::EConstruct(_, args) | Op::EData(_, args) => expressions.extend(args),
                Op::ECollect => e.may_explicitly_collect = true,
                Op::EInt(_) | Op::EBool(_) | Op::EUnit | Op::EString(_) | Op::ELocal(_) => {}
            }
        }
        effects.push(e);
    }
    // Each bit can change from false to true once per function. Reverse edges
    // propagate changes across SCCs without trusting provisional summaries.
    let mut queue = (0..effects.len()).collect::<VecDeque<_>>();
    let mut queued = vec![true; effects.len()];
    while let Some(callee) = queue.pop_front() {
        queued[callee] = false;
        let summary = effects[callee].clone();
        for caller in &callers[callee] {
            if effects[*caller].include(&summary) && !queued[*caller] {
                queued[*caller] = true;
                queue.push_back(*caller);
            }
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgx2_runtime::default_managed::model::Loc;

    #[test]
    fn literal_budgets_and_both_assignment_tags_do_not_visit_unrelated_operations() {
        let program = super::super::compile_source("fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            42\n        Result.Err(e):\n            0\n").unwrap();
        let mut memory = program.runtime().clone();
        memory.operations.clear();
        memory.program.expressions[0].op = Op::ECollect;
        memory.program.expressions[1].op = Op::ECollect;
        let mut scalar = memory.program.expressions[0].clone();
        scalar.op = Op::EInt(42);
        let rhs = memory.program.expressions.len();
        memory.program.expressions.push(scalar);
        for f in &mut memory.program.functions {
            f.requires.clear();
            f.ensures.clear();
            f.body.clear();
            f.effects = Default::default();
        }
        memory.program.functions[memory.program.entry].body = vec![Statement::SGuard(
            1 << 40,
            Loc { start: 0, end: 0 },
            vec![Statement::SAssign(0, 0, rhs), Statement::SAssign(0, 1, rhs)],
        )];
        let summary = &infer(&memory)[memory.program.entry];
        assert!(summary.may_enter_guard);
        assert!(!summary.may_explicitly_collect);
        assert!(!summary.may_allocate_managed);
        assert!(!summary.may_transfer_graph);
    }
}
