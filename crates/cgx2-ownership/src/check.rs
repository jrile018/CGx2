use std::collections::{BTreeMap, BTreeSet};

use cgx2_syntax::Span;

use crate::{Op, OwnershipDiagnostic};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValueState {
    Owned,
    Managed(String),
    Dead,
    MaybeOwned,
    MaybeManaged(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Borrow {
    value: String,
    mutable: bool,
    span: Span,
}

#[derive(Clone, Default)]
struct Checker {
    values: BTreeMap<String, ValueState>,
    regions: BTreeMap<String, bool>,
    borrows: BTreeMap<String, Borrow>,
    no_gc: Vec<Span>,
    diagnostics: Vec<OwnershipDiagnostic>,
}

impl Checker {
    fn report(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(OwnershipDiagnostic {
            code,
            message: message.into(),
            span,
        });
    }

    fn create(&mut self, name: &str, state: ValueState, span: Span) -> bool {
        if self.values.contains_key(name) {
            self.report("CGO001", format!("duplicate value '{name}'"), span);
            return false;
        }
        self.values.insert(name.to_owned(), state);
        true
    }

    fn live(&mut self, name: &str, span: Span) -> bool {
        match self.values.get(name) {
            Some(ValueState::Owned | ValueState::Managed(_)) => true,
            _ => {
                self.report(
                    "CGO001",
                    format!("value '{name}' is unknown or unavailable"),
                    span,
                );
                false
            }
        }
    }

    fn owned(&mut self, name: &str, span: Span, code: &'static str) -> bool {
        if self.values.get(name) == Some(&ValueState::Owned) {
            true
        } else {
            self.report(code, format!("value '{name}' is not live owned"), span);
            false
        }
    }

    fn unborrowed(&mut self, name: &str, span: Span) -> bool {
        if self.borrows.values().any(|borrow| borrow.value == name) {
            self.report("CGO002", format!("value '{name}' is borrowed"), span);
            false
        } else {
            true
        }
    }

    fn open_region(&mut self, region: &str, span: Span) -> bool {
        if self.regions.get(region) == Some(&true) {
            true
        } else {
            self.report("CGO003", format!("region '{region}' is not open"), span);
            false
        }
    }

    fn borrow(&mut self, value: &str, token: &str, mutable: bool, span: Span) {
        if !self.live(value, span) {
            return;
        }
        if self.borrows.contains_key(token) {
            self.report("CGO002", format!("borrow token '{token}' is active"), span);
            return;
        }
        if self
            .borrows
            .values()
            .any(|borrow| borrow.value == value && (mutable || borrow.mutable))
        {
            self.report("CGO002", format!("conflicting borrow of '{value}'"), span);
            return;
        }
        self.borrows.insert(
            token.to_owned(),
            Borrow {
                value: value.to_owned(),
                mutable,
                span,
            },
        );
    }

    fn check_if(&mut self, then_ops: &[Op], else_ops: &[Op], span: Span) {
        let mut then_branch = self.clone();
        let mut else_branch = self.clone();
        then_branch.diagnostics.clear();
        else_branch.diagnostics.clear();
        then_branch.check(then_ops);
        else_branch.check(else_ops);

        for branch in [&mut then_branch, &mut else_branch] {
            let leaked: Vec<_> = branch
                .borrows
                .iter()
                .filter(|(token, _)| !self.borrows.contains_key(*token))
                .map(|(token, borrow)| (token.clone(), borrow.span))
                .collect();
            for (token, at) in leaked {
                branch.report(
                    "CGO002",
                    format!("borrow token '{token}' escapes branch"),
                    at,
                );
                branch.borrows.remove(&token);
            }
            if !self.no_gc.starts_with(&branch.no_gc) {
                branch.report("CGO005", "no_gc guard escapes branch", span);
            }
        }

        self.diagnostics
            .extend(then_branch.diagnostics.iter().cloned());
        self.diagnostics
            .extend(else_branch.diagnostics.iter().cloned());

        if then_branch.regions == else_branch.regions {
            self.regions = then_branch.regions;
        } else {
            self.report("CGO005", "regions differ across branches", span);
        }
        if then_branch.borrows == else_branch.borrows {
            self.borrows = then_branch.borrows;
        } else {
            self.report("CGO005", "borrow state differs across branches", span);
        }
        if then_branch.no_gc == else_branch.no_gc {
            self.no_gc = then_branch.no_gc;
        } else {
            self.report("CGO005", "no_gc state differs across branches", span);
        }

        let names: BTreeSet<_> = then_branch
            .values
            .keys()
            .chain(else_branch.values.keys())
            .cloned()
            .collect();
        let mut joined = BTreeMap::new();
        for name in names {
            match join_value(then_branch.values.get(&name), else_branch.values.get(&name)) {
                Ok(Some(state)) => {
                    joined.insert(name, state);
                }
                Ok(None) => {}
                Err(()) => {
                    self.report(
                        "CGO005",
                        format!("value '{name}' has incompatible branch states"),
                        span,
                    );
                    joined.insert(name, ValueState::Dead);
                }
            }
        }
        self.values = joined;
    }

    fn check_loop(&mut self, body: &[Op], span: Span) {
        let mut backedge = self.clone();
        backedge.diagnostics.clear();
        backedge.check(body);
        // Dead scoped entries are irrelevant, but every incoming entry (including
        // unavailable names) and all region/borrow/guard facts must be unchanged.
        let outer_unchanged = self
            .values
            .iter()
            .all(|(name, state)| backedge.values.get(name) == Some(state));
        let locals_finished = backedge
            .values
            .iter()
            .all(|(name, state)| self.values.contains_key(name) || *state == ValueState::Dead);
        if !outer_unchanged
            || !locals_finished
            || self.regions != backedge.regions
            || self.borrows != backedge.borrows
            || self.no_gc != backedge.no_gc
        {
            backedge.report("CGO005", "loop backedge must preserve outer ownership, regions, borrows and no_gc state; locals must finish", span);
        }
        self.diagnostics.extend(backedge.diagnostics);
        // The zero-trip join is exactly the incoming state, never the body state.
    }

    fn check_end(&mut self) {
        let leaked: Vec<_> = self
            .borrows
            .iter()
            .map(|(token, borrow)| (token.clone(), borrow.span))
            .collect();
        for (token, span) in leaked {
            self.report(
                "CGO002",
                format!("borrow token '{token}' was not ended"),
                span,
            );
        }
        let open_guards = self.no_gc.clone();
        for span in open_guards {
            self.report("CGO005", "no_gc guard was not exited", span);
        }
    }

    fn check(&mut self, ops: &[Op]) {
        for op in ops {
            match op {
                Op::CreateOwned { value, span } => {
                    self.create(value, ValueState::Owned, *span);
                }
                Op::Use { value, span } => {
                    if self.live(value, *span)
                        && self
                            .borrows
                            .values()
                            .any(|borrow| borrow.value == *value && borrow.mutable)
                    {
                        self.report(
                            "CGO002",
                            format!("value '{value}' is mutably borrowed"),
                            *span,
                        );
                    }
                }
                Op::Move { from, to, span } => {
                    if self.owned(from, *span, "CGO001")
                        && self.unborrowed(from, *span)
                        && self.create(to, ValueState::Owned, *span)
                    {
                        self.values.insert(from.clone(), ValueState::Dead);
                    }
                }
                Op::DropValue { value, span } => {
                    if self.live(value, *span) && self.unborrowed(value, *span) {
                        self.values.insert(value.clone(), ValueState::Dead);
                    }
                }
                Op::BorrowShared { value, token, span } => self.borrow(value, token, false, *span),
                Op::BorrowMut { value, token, span } => self.borrow(value, token, true, *span),
                Op::EndBorrow { token, span } => {
                    if self.borrows.remove(token).is_none() {
                        self.report("CGO002", format!("unknown borrow token '{token}'"), *span);
                    }
                }
                Op::OpenRegion { region, span } => {
                    if self.regions.contains_key(region) {
                        self.report("CGO003", format!("duplicate region '{region}'"), *span);
                    } else {
                        self.regions.insert(region.clone(), true);
                    }
                }
                Op::CreateManaged {
                    value,
                    region,
                    span,
                } => {
                    if self.open_region(region, *span) {
                        self.create(value, ValueState::Managed(region.clone()), *span);
                    }
                }
                Op::Promote {
                    from,
                    to,
                    region,
                    span,
                } => {
                    if self.owned(from, *span, "CGO003")
                        && self.unborrowed(from, *span)
                        && self.open_region(region, *span)
                        && self.create(to, ValueState::Managed(region.clone()), *span)
                    {
                        self.values.insert(from.clone(), ValueState::Dead);
                    }
                }
                Op::CopyToOwned { from, to, span } => {
                    if matches!(self.values.get(from), Some(ValueState::Managed(_))) {
                        if self
                            .borrows
                            .values()
                            .any(|borrow| borrow.value == *from && borrow.mutable)
                        {
                            self.report(
                                "CGO002",
                                format!("value '{from}' is mutably borrowed"),
                                *span,
                            );
                        } else {
                            self.create(to, ValueState::Owned, *span);
                        }
                    } else {
                        self.report(
                            "CGO003",
                            format!("value '{from}' is not live managed"),
                            *span,
                        );
                    }
                }
                Op::CloseRegion { region, span } => {
                    if self.open_region(region, *span) {
                        if self.values.values().any(|state| matches!(state, ValueState::Managed(r) | ValueState::MaybeManaged(r) if r == region)) {
                            self.report("CGO003", format!("region '{region}' has live handles"), *span);
                        } else {
                            self.regions.insert(region.clone(), false);
                        }
                    }
                }
                Op::EnterNoGc { span } => self.no_gc.push(*span),
                Op::ExitNoGc { span } => {
                    if self.no_gc.pop().is_none() {
                        self.report("CGO005", "no_gc exit has no matching entry", *span);
                    }
                }
                Op::Collect { region, span } => {
                    if !self.no_gc.is_empty() {
                        self.report("CGO004", "collection is forbidden inside no_gc", *span);
                    } else {
                        self.open_region(region, *span);
                    }
                }
                Op::Loop { body, span } => self.check_loop(body, *span),
                Op::If {
                    then_ops,
                    else_ops,
                    span,
                } => self.check_if(then_ops, else_ops, *span),
                Op::Unsupported { feature, span } => {
                    self.report("CGO005", format!("unsupported operation: {feature}"), *span);
                }
            }
        }
    }
}

fn join_value(
    left: Option<&ValueState>,
    right: Option<&ValueState>,
) -> Result<Option<ValueState>, ()> {
    use ValueState::{Dead, Managed, MaybeManaged, MaybeOwned, Owned};
    match (left, right) {
        (None, None) => Ok(None),
        (Some(a), Some(b)) if a == b => Ok(Some(a.clone())),
        (Some(Owned | MaybeOwned), Some(Dead | MaybeOwned | Owned))
        | (Some(Dead), Some(Owned | MaybeOwned))
        | (Some(Owned | MaybeOwned), None)
        | (None, Some(Owned | MaybeOwned)) => Ok(Some(MaybeOwned)),
        (Some(Managed(region) | MaybeManaged(region)), Some(Dead))
        | (Some(Dead), Some(Managed(region) | MaybeManaged(region)))
        | (Some(Managed(region) | MaybeManaged(region)), None)
        | (None, Some(Managed(region) | MaybeManaged(region))) => {
            Ok(Some(MaybeManaged(region.clone())))
        }
        (Some(Managed(a) | MaybeManaged(a)), Some(Managed(b) | MaybeManaged(b))) if a == b => {
            Ok(Some(MaybeManaged(a.clone())))
        }
        (Some(Dead), None) | (None, Some(Dead)) => Ok(Some(Dead)),
        _ => Err(()),
    }
}

pub fn check_ops(ops: &[Op]) -> Result<(), Vec<OwnershipDiagnostic>> {
    let mut checker = Checker::default();
    checker.check(ops);
    checker.check_end();
    if checker.diagnostics.is_empty() {
        Ok(())
    } else {
        Err(checker.diagnostics)
    }
}
