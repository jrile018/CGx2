//! Same byte-storage interpreter compiled into both source execution and native output.
use super::model_v2::*;
use cgx2_runtime::{AllocationError, OwnedBytes, Session};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scalar {
    I64(i64),
    Bool(bool),
    Record {
        name: String,
        fields: Vec<(String, Scalar)>,
    },
    Unit,
}
impl std::fmt::Display for Scalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Record { name, fields } => {
                write!(f, "{name}(")?;
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                write!(f, ")")
            }
            Self::Unit => Ok(()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    pub result_ok: usize,
    pub result_err: usize,
    pub allocation_failures: usize,
    pub guard_entries: usize,
    pub guard_exits: usize,
    pub guard_failures: usize,
    pub charged_bytes: u128,
    pub owned_charged_bytes: u128,
    pub guard_charged_bytes: u128,
    pub collection_count: u64,
    pub active_guards_at_exit: usize,
    pub allocations: usize,
    pub releases: usize,
    pub live_bytes: usize,
    pub allocated_bytes: usize,
    pub released_bytes: usize,
    pub peak_live_bytes: usize,
    pub reads: usize,
    pub writes: usize,
}
impl Evidence {
    pub fn json(&self) -> String {
        format!("{{\"schema\":1,\"allocations\":{},\"releases\":{},\"live_bytes\":{},\"allocated_bytes\":{},\"released_bytes\":{},\"peak_live_bytes\":{},\"reads\":{},\"writes\":{}}}", self.allocations, self.releases, self.live_bytes, self.allocated_bytes, self.released_bytes, self.peak_live_bytes, self.reads, self.writes)
    }
    pub fn json_v2(&self) -> String {
        format!(concat!("{{\"schema\":2,\"allocations\":{},\"releases\":{},\"live_bytes\":{},\"allocated_bytes\":{},\"released_bytes\":{},\"peak_live_bytes\":{},\"reads\":{},\"writes\":{},",
            "\"result_ok\":{},\"result_err\":{},\"allocation_failures\":{},\"guard_entries\":{},\"guard_exits\":{},\"guard_failures\":{},\"charged_bytes\":{},\"owned_charged_bytes\":{},\"guard_charged_bytes\":{},\"collection_count\":{},\"active_guards_at_exit\":{}}}"),
            self.allocations, self.releases, self.live_bytes, self.allocated_bytes, self.released_bytes, self.peak_live_bytes, self.reads, self.writes,
            self.result_ok, self.result_err, self.allocation_failures, self.guard_entries, self.guard_exits, self.guard_failures, self.charged_bytes, self.owned_charged_bytes, self.guard_charged_bytes, self.collection_count, self.active_guards_at_exit)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub result: Result<Scalar, Failure>,
    pub evidence: Evidence,
}
enum Value {
    Scalar(Scalar),
    Record(usize, Vec<Scalar>),
    Owner(usize),
    OwnedResult(Result<usize, AllocationError>),
    Error(AllocationError),
    Loan(usize),
}
struct Engine<'a, 's> {
    model: &'a Model,
    session: &'s mut Session,
    storage: &'s mut Vec<Option<(usize, OwnedBytes)>>,
    evidence: &'s mut Evidence,
    steps: &'s mut usize,
    bridge: &'s mut dyn ManagedBridge,
}
type Eval<T> = Result<T, Failure>;
fn fail(span: SourceSpan, message: impl Into<String>) -> Failure {
    Failure {
        code: "CGR080",
        message: message.into(),
        span,
    }
}
pub(super) fn run(model: &Model) -> Execution {
    match Session::try_new() {
        Ok(session) => run_with_session(model, session),
        Err(e) => Execution {
            result: Err(fail(
                model.functions[model.entry].span,
                format!("session allocation failed: {e:?}"),
            )),
            evidence: Evidence::default(),
        },
    }
}
pub(super) fn run_with_session(model: &Model, mut session: Session) -> Execution {
    run_with_bridge(model, &mut session, &mut NoBridge)
}
pub trait ManagedBridge {
    fn invoke(
        &mut self,
        name: &str,
        arguments: &[Scalar],
        session: &mut Session,
        remaining_steps: usize,
        depth: usize,
    ) -> Option<(Result<Scalar, Failure>, usize)>;
}
struct NoBridge;
impl ManagedBridge for NoBridge {
    fn invoke(
        &mut self,
        _: &str,
        _: &[Scalar],
        _: &mut Session,
        _: usize,
        _: usize,
    ) -> Option<(Result<Scalar, Failure>, usize)> {
        None
    }
}
pub fn run_with_bridge(
    model: &Model,
    session: &mut Session,
    bridge: &mut dyn ManagedBridge,
) -> Execution {
    let mut storage = vec![];
    let mut evidence = Evidence::default();
    let mut steps = 0;
    let mut e = Engine {
        model,
        session,
        storage: &mut storage,
        evidence: &mut evidence,
        steps: &mut steps,
        bridge,
    };
    let result = e.call(model.entry, vec![], 0).and_then(|v| match v {
        Value::Scalar(v) => Ok(v),
        Value::Record(layout, values) => Ok(Scalar::Record {
            name: model.layouts[layout].name.clone(),
            fields: model.layouts[layout]
                .fields
                .iter()
                .zip(values)
                .map(|(f, v)| (f.name.clone(), v))
                .collect(),
        }),
        _ => Err(fail(
            model.functions[model.entry].span,
            "main must return a copied value or Unit",
        )),
    });
    for id in 0..e.storage.len() {
        e.release(id);
    }
    let observation = e.session.memory_observation_v2();
    e.evidence.charged_bytes = observation.charged_bytes;
    e.evidence.collection_count = observation.collection_count;
    e.evidence.active_guards_at_exit = observation.active_guards;
    Execution { result, evidence }
}
impl Engine<'_, '_> {
    fn tick(&mut self, span: SourceSpan) -> Eval<()> {
        *self.steps += 1;
        if *self.steps > 1_000_000 {
            Err(fail(span, "ordinary execution step limit"))
        } else {
            Ok(())
        }
    }
    fn release(&mut self, id: usize) {
        if let Some((_, bytes)) = self.storage[id].take() {
            self.evidence.releases += 1;
            self.evidence.live_bytes -= bytes.len();
            self.evidence.released_bytes += bytes.len();
            drop(bytes);
        }
    }
    fn dispose(&mut self, value: Value) {
        if let Value::Owner(id) | Value::OwnedResult(Ok(id)) = value {
            self.release(id);
        }
    }
    fn call(&mut self, id: usize, arguments: Vec<Value>, depth: usize) -> Eval<Value> {
        let f = &self.model.functions[id];
        let copied = arguments
            .iter()
            .map(|v| match v {
                Value::Scalar(v) => Some(v.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        if let Some(arguments) = copied {
            if let Some((result, used)) = self.bridge.invoke(
                &f.name,
                &arguments,
                self.session,
                1_000_000usize.saturating_sub(*self.steps),
                depth,
            ) {
                *self.steps = self.steps.saturating_add(used);
                return result.map(Value::Scalar);
            }
        }
        if depth >= 64 {
            return Err(fail(f.span, "ordinary call depth limit"));
        }
        let mut slots: Vec<Option<Value>> = (0..f.locals.len()).map(|_| None).collect();
        for (slot, value) in slots.iter_mut().zip(arguments) {
            *slot = Some(value);
        }
        let result = self
            .block(&f.body, &mut slots, depth)
            .map(|v| v.unwrap_or(Value::Scalar(Scalar::Unit)));
        for v in slots.into_iter().flatten() {
            self.dispose(v);
        }
        result
    }
    fn block(
        &mut self,
        body: &[Statement],
        slots: &mut [Option<Value>],
        depth: usize,
    ) -> Eval<Option<Value>> {
        let mut locals = Vec::new();
        let result = (|| {
            for s in body {
                self.tick(s.span)?;
                match &s.kind {
                    StatementKind::NoGc { budget, body } => {
                        let before = self.session.memory_observation_v2().charged_bytes;
                        let mut guard = match self.session.try_no_gc(*budget) {
                            Ok(guard) => guard,
                            Err(error) => {
                                self.evidence.guard_failures += 1;
                                return Err(fail(s.span, format!("no_gc entry failed: {error:?}")));
                            }
                        };
                        self.evidence.guard_entries += 1;
                        self.evidence.guard_charged_bytes +=
                            guard.memory_observation_v2().charged_bytes - before;
                        let result = Engine {
                            model: self.model,
                            session: &mut guard,
                            storage: self.storage,
                            evidence: self.evidence,
                            steps: self.steps,
                            bridge: self.bridge,
                        }
                        .block(body, slots, depth);
                        drop(guard);
                        self.evidence.guard_exits += 1;
                        if let Some(value) = result? {
                            return Ok(Some(value));
                        }
                    }
                    StatementKind::MatchResult {
                        scrutinee,
                        ok_local,
                        err_local,
                        ok_body,
                        err_body,
                    } => {
                        let Value::OwnedResult(result) = self.expr(scrutinee, slots, depth)? else {
                            return Err(fail(s.span, "expected conditional owner"));
                        };
                        let (local, value, body) = match result {
                            Ok(id) => (*ok_local, Value::Owner(id), ok_body),
                            Err(error) => (*err_local, Value::Error(error), err_body),
                        };
                        slots[local] = Some(value);
                        let result = self.block(body, slots, depth);
                        if let Some(value) = slots[local].take() {
                            self.dispose(value);
                        }
                        if let Some(value) = result? {
                            return Ok(Some(value));
                        }
                    }
                    StatementKind::MatchError { scrutinee, arms } => {
                        let Value::Error(error) = self.expr(scrutinee, slots, depth)? else {
                            return Err(fail(s.span, "expected allocation error"));
                        };
                        let arm = match error {
                            AllocationError::BudgetExceeded => 0,
                            AllocationError::RegionLimit => 1,
                            AllocationError::HostAllocation => 2,
                            AllocationError::InvalidRegion => 3,
                            AllocationError::ShapeLimit => 4,
                        };
                        if let Some(value) = self.block(&arms[arm], slots, depth)? {
                            return Ok(Some(value));
                        }
                    }
                    StatementKind::Let { local, value } => {
                        let v = self.expr(value, slots, depth)?;
                        slots[*local] = Some(v);
                        locals.push(*local);
                    }
                    StatementKind::Set {
                        local,
                        field,
                        value,
                        add,
                    } => {
                        let value = self.expr(value, slots, depth)?;
                        if let Value::Error(error) = value {
                            if field.is_some() || *add {
                                return Err(fail(s.span, "invalid copied-error assignment"));
                            }
                            slots[*local] = Some(Value::Error(error));
                            continue;
                        }
                        let Value::Scalar(mut scalar) = value else {
                            return Err(fail(s.span, "non-scalar assignment"));
                        };
                        if let Some(field) = field {
                            let id = Self::storage_id(slots, *local, s.span)?;
                            let layout = self.storage[id]
                                .as_ref()
                                .ok_or_else(|| fail(s.span, "released storage"))?
                                .0;
                            if *add {
                                scalar = arithmetic(
                                    0,
                                    self.read(id, layout, *field, s.span)?,
                                    scalar,
                                    s.span,
                                )?;
                            }
                            self.write(id, layout, *field, &scalar, s.span)?;
                        } else {
                            if *add {
                                let Some(Value::Scalar(old)) = &slots[*local] else {
                                    return Err(fail(s.span, "uninitialized scalar"));
                                };
                                scalar = arithmetic(0, old.clone(), scalar, s.span)?;
                            }
                            slots[*local] = Some(Value::Scalar(scalar));
                        }
                    }
                    StatementKind::Evaluate(e) => {
                        let v = self.expr(e, slots, depth)?;
                        self.dispose(v);
                    }
                    StatementKind::Return(e) => return self.expr(e, slots, depth).map(Some),
                    StatementKind::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        let Value::Scalar(Scalar::Bool(test)) =
                            self.expr(condition, slots, depth)?
                        else {
                            return Err(fail(s.span, "non-Boolean condition"));
                        };
                        if let Some(v) =
                            self.block(if test { then_body } else { else_body }, slots, depth)?
                        {
                            return Ok(Some(v));
                        }
                    }
                }
            }
            Ok(None)
        })();
        for local in locals {
            if let Some(v) = slots[local].take() {
                self.dispose(v);
            }
        }
        result
    }
    fn storage_id(slots: &[Option<Value>], local: usize, span: SourceSpan) -> Eval<usize> {
        match &slots[local] {
            Some(Value::Owner(id) | Value::Loan(id)) => Ok(*id),
            _ => Err(fail(span, "owner is no longer live")),
        }
    }
    fn expr(&mut self, e: &Expression, slots: &mut [Option<Value>], depth: usize) -> Eval<Value> {
        self.tick(e.span)?;
        Ok(match &e.kind {
            ExpressionKind::I64(v) => Value::Scalar(Scalar::I64(*v)),
            ExpressionKind::Bool(v) => Value::Scalar(Scalar::Bool(*v)),
            ExpressionKind::Unit => Value::Scalar(Scalar::Unit),
            ExpressionKind::Local(i) => match slots[*i].as_ref() {
                Some(Value::Owner(_) | Value::OwnedResult(_)) => slots[*i]
                    .take()
                    .ok_or_else(|| fail(e.span, "owner moved"))?,
                Some(Value::Loan(id)) => Value::Loan(*id),
                Some(Value::Error(error)) => Value::Error(*error),
                Some(Value::Scalar(s)) => Value::Scalar(s.clone()),
                Some(Value::Record(l, fields)) => Value::Record(*l, fields.clone()),
                None => return Err(fail(e.span, "binding moved or uninitialized")),
            },
            ExpressionKind::Field { local, field } => {
                match &slots[*local] {
                    Some(Value::Record(_, fields)) => Value::Scalar(fields[*field].clone()),
                    _ => {
                        let id = Self::storage_id(slots, *local, e.span)?;
                        // The layout identity is encoded in the field expression by its base local.
                        let l = self.storage[id]
                            .as_ref()
                            .ok_or_else(|| fail(e.span, "released storage"))?
                            .0;
                        Value::Scalar(self.read(id, l, *field, e.span)?)
                    }
                }
            }
            ExpressionKind::Record {
                layout,
                fields,
                order,
            } => {
                let mut values = vec![Scalar::Unit; fields.len()];
                for (f, index) in fields.iter().zip(order) {
                    let Value::Scalar(s) = self.expr(f, slots, depth)? else {
                        return Err(fail(f.span, "record field must be copied"));
                    };
                    values[*index] = s;
                }
                Value::Record(*layout, values)
            }
            ExpressionKind::Owned(value) | ExpressionKind::TryOwned(value) => {
                let Value::Record(layout, fields) = self.expr(value, slots, depth)? else {
                    return Err(fail(e.span, "owned requires a scalar record"));
                };
                let recoverable = matches!(e.kind, ExpressionKind::TryOwned(_));
                match self.allocate(layout, &fields, e.span) {
                    Ok(id) if recoverable => {
                        self.evidence.result_ok += 1;
                        Value::OwnedResult(Ok(id))
                    }
                    Ok(id) => Value::Owner(id),
                    Err(error) if recoverable => {
                        self.evidence.result_err += 1;
                        Value::OwnedResult(Err(error))
                    }
                    Err(error) => {
                        return Err(fail(e.span, format!("owned allocation failed: {error:?}")))
                    }
                }
            }
            ExpressionKind::Borrow { local } => {
                Value::Loan(Self::storage_id(slots, *local, e.span)?)
            }
            ExpressionKind::Call {
                function,
                arguments,
            } => {
                let mut args = Vec::new();
                for a in arguments {
                    match self.expr(a, slots, depth) {
                        Ok(v) => args.push(v),
                        Err(error) => {
                            for v in args {
                                self.dispose(v);
                            }
                            return Err(error);
                        }
                    }
                }
                self.call(*function, args, depth + 1)?
            }
            ExpressionKind::Binary { op, left, right } => {
                let Value::Scalar(a) = self.expr(left, slots, depth)? else {
                    return Err(fail(e.span, "non-scalar arithmetic"));
                };
                let Value::Scalar(b) = self.expr(right, slots, depth)? else {
                    return Err(fail(e.span, "non-scalar arithmetic"));
                };
                Value::Scalar(arithmetic(*op, a, b, e.span)?)
            }
        })
    }
    fn allocate(
        &mut self,
        layout: usize,
        fields: &[Scalar],
        at: SourceSpan,
    ) -> Result<usize, AllocationError> {
        let len = self.model.layouts[layout].size;
        let before = self.session.memory_observation_v2().charged_bytes;
        let bytes = match self.session.alloc_owned_bytes(len) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.evidence.allocation_failures += 1;
                return Err(error);
            }
        };
        self.evidence.owned_charged_bytes +=
            self.session.memory_observation_v2().charged_bytes - before;
        let id = self
            .storage
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.storage.len());
        if id == self.storage.len() {
            self.storage.push(Some((layout, bytes)));
        } else {
            self.storage[id] = Some((layout, bytes));
        }
        self.evidence.allocations += 1;
        self.evidence.allocated_bytes += len;
        self.evidence.live_bytes += len;
        self.evidence.peak_live_bytes = self.evidence.peak_live_bytes.max(self.evidence.live_bytes);
        for (i, scalar) in fields.iter().enumerate() {
            // Authenticated scalar layouts/values make encoding total.
            self.write(id, layout, i, scalar, at)
                .expect("checked scalar record encoding");
        }
        Ok(id)
    }
    fn read(&mut self, id: usize, layout: usize, field: usize, span: SourceSpan) -> Eval<Scalar> {
        let f = &self.model.layouts[layout].fields[field];
        let bytes = self.storage[id]
            .as_ref()
            .ok_or_else(|| fail(span, "released storage"))?
            .1
            .as_slice();
        self.evidence.reads += 1;
        match f.ty {
            Ty::I64 => Ok(Scalar::I64(i64::from_le_bytes(
                bytes
                    .get(f.offset..f.offset + 8)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| fail(span, "invalid i64 field layout"))?,
            ))),
            Ty::Bool => match bytes.get(f.offset) {
                Some(0) => Ok(Scalar::Bool(false)),
                Some(1) => Ok(Scalar::Bool(true)),
                _ => Err(fail(span, "invalid Bool byte")),
            },
            _ => Err(fail(span, "invalid scalar layout")),
        }
    }
    fn write(
        &mut self,
        id: usize,
        layout: usize,
        field: usize,
        value: &Scalar,
        span: SourceSpan,
    ) -> Eval<()> {
        let f = &self.model.layouts[layout].fields[field];
        let bytes = self.storage[id]
            .as_mut()
            .ok_or_else(|| fail(span, "released storage"))?
            .1
            .as_mut_slice();
        match (f.ty, value) {
            (Ty::I64, Scalar::I64(n)) => bytes
                .get_mut(f.offset..f.offset + 8)
                .ok_or_else(|| fail(span, "invalid i64 field layout"))?
                .copy_from_slice(&n.to_le_bytes()),
            (Ty::Bool, Scalar::Bool(b)) => {
                *bytes
                    .get_mut(f.offset)
                    .ok_or_else(|| fail(span, "invalid Bool field layout"))? = u8::from(*b)
            }
            _ => return Err(fail(span, "field encoding type mismatch")),
        }
        self.evidence.writes += 1;
        Ok(())
    }
}
fn arithmetic(op: u8, a: Scalar, b: Scalar, span: SourceSpan) -> Eval<Scalar> {
    if op >= 4 {
        let result = match (&a, &b) {
            (Scalar::I64(a), Scalar::I64(b)) => match op {
                4 => a == b,
                5 => a != b,
                6 => a < b,
                7 => a <= b,
                8 => a > b,
                9 => a >= b,
                _ => return Err(fail(span, "invalid operator")),
            },
            (Scalar::Bool(a), Scalar::Bool(b)) if op <= 5 => {
                if op == 4 {
                    a == b
                } else {
                    a != b
                }
            }
            _ => return Err(fail(span, "comparison type mismatch")),
        };
        return Ok(Scalar::Bool(result));
    }
    let (Scalar::I64(a), Scalar::I64(b)) = (a, b) else {
        return Err(fail(span, "arithmetic requires i64"));
    };
    let n = match op {
        0 => a.checked_add(b),
        1 => a.checked_sub(b),
        2 => a.checked_mul(b),
        3 => a.checked_div(b),
        _ => None,
    };
    n.map(Scalar::I64)
        .ok_or_else(|| fail(span, "checked integer arithmetic failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_little_endian_and_bool_validity_use_real_owned_bytes() {
        let source = "record R:\n    x: i64\n    yes: Bool\nfn main() -> i64:\n    let p = owned(R(x: 0, yes: true))\n    p.x\n";
        let p = crate::ordinary_owned::compile_source_v2(source).unwrap();
        let model = p.model();
        let mut session = Session::try_new().unwrap();
        let storage = session.alloc_owned_bytes(9).unwrap();
        let mut engine = Engine {
            model,
            session: &mut session,
            storage: &mut vec![Some((0, storage))],
            evidence: &mut Evidence::default(),
            steps: &mut 0,
            bridge: &mut NoBridge,
        };
        let span = model.layouts[0].span;
        engine.write(0, 0, 0, &Scalar::I64(i64::MIN), span).unwrap();
        assert_eq!(
            engine.storage[0].as_ref().unwrap().1.as_slice()[..8],
            i64::MIN.to_le_bytes()
        );
        assert_eq!(engine.read(0, 0, 0, span).unwrap(), Scalar::I64(i64::MIN));
        engine.storage[0].as_mut().unwrap().1.as_mut_slice()[8] = 2;
        assert_eq!(
            engine.read(0, 0, 1, span).unwrap_err().message,
            "invalid Bool byte"
        );
        assert!(arithmetic(3, Scalar::I64(i64::MIN), Scalar::I64(-1), span).is_err());
    }
}
