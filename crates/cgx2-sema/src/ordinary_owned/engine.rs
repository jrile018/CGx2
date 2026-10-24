//! Same byte-storage interpreter compiled into both source execution and native output.
use super::model::*;
use cgx2_runtime::{OwnedBytes, Session};

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
        format!(
            "{{\"schema\":1,\"allocations\":{},\"releases\":{},\"live_bytes\":{},\"allocated_bytes\":{},\"released_bytes\":{},\"peak_live_bytes\":{},\"reads\":{},\"writes\":{}}}",
            self.allocations,
            self.releases,
            self.live_bytes,
            self.allocated_bytes,
            self.released_bytes,
            self.peak_live_bytes,
            self.reads,
            self.writes
        )
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
    Loan(usize),
}
struct Engine<'a> {
    model: &'a Model,
    session: Session,
    storage: Vec<Option<(usize, OwnedBytes)>>,
    evidence: Evidence,
    steps: usize,
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
pub(super) fn run_with_session(model: &Model, session: Session) -> Execution {
    let mut e = Engine {
        model,
        session,
        storage: vec![],
        evidence: Evidence::default(),
        steps: 0,
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
    // Also releases temporaries on allocation/arithmetic/argument evaluation failure.
    for id in 0..e.storage.len() {
        e.release(id);
    }
    Execution {
        result,
        evidence: e.evidence,
    }
}
impl Engine<'_> {
    fn tick(&mut self, span: SourceSpan) -> Eval<()> {
        self.steps += 1;
        if self.steps > 1_000_000 {
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
        if let Value::Owner(id) = value {
            self.release(id);
        }
    }
    fn call(&mut self, id: usize, arguments: Vec<Value>, depth: usize) -> Eval<Value> {
        let f = &self.model.functions[id];
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
                Some(Value::Owner(_)) => slots[*i]
                    .take()
                    .ok_or_else(|| fail(e.span, "owner moved"))?,
                Some(Value::Loan(id)) => Value::Loan(*id),
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
            ExpressionKind::Owned(value) => {
                let Value::Record(layout, fields) = self.expr(value, slots, depth)? else {
                    return Err(fail(e.span, "owned requires a scalar record"));
                };
                let len = self.model.layouts[layout].size;
                let bytes = self
                    .session
                    .alloc_owned_bytes(len)
                    .map_err(|a| fail(e.span, format!("owned allocation failed: {a:?}")))?;
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
                self.evidence.peak_live_bytes =
                    self.evidence.peak_live_bytes.max(self.evidence.live_bytes);
                for (i, scalar) in fields.iter().enumerate() {
                    self.write(id, layout, i, scalar, e.span)?;
                }
                Value::Owner(id)
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
                    args.push(self.expr(a, slots, depth)?);
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
        let p = crate::ordinary_owned::compile_source(source).unwrap();
        let model = p.model();
        let mut session = Session::try_new().unwrap();
        let storage = session.alloc_owned_bytes(9).unwrap();
        let mut engine = Engine {
            model,
            session,
            storage: vec![Some((0, storage))],
            evidence: Evidence::default(),
            steps: 0,
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
