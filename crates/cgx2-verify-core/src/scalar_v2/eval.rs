use std::collections::BTreeMap;

use super::Expr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Bool(bool),
    I64(i64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvalError {
    Overflow,
    UnknownVariable(String),
    SortMismatch,
    Unsupported(String),
}

pub fn eval(expr: &Expr, values: &BTreeMap<String, Value>) -> Result<Value, EvalError> {
    use Expr::*;
    match expr {
        If(guard, yes, no) => {
            if as_bool(eval(guard, values)?)? {
                eval(yes, values)
            } else {
                eval(no, values)
            }
        }
        Let(name, value, body) => {
            let value = eval(value, values)?;
            let mut scope = values.clone();
            scope.insert(name.clone(), value);
            eval(body, &scope)
        }
        Mul(a, b) => Ok(Value::I64(
            as_i64(eval(a, values)?)?
                .checked_mul(as_i64(eval(b, values)?)?)
                .ok_or(EvalError::Overflow)?,
        )),
        Div(a, b) => Ok(Value::I64(
            as_i64(eval(a, values)?)?
                .checked_div(as_i64(eval(b, values)?)?)
                .ok_or(EvalError::Overflow)?,
        )),
        Bool(v) => Ok(Value::Bool(*v)),
        I64(v) => Ok(Value::I64(*v)),
        Var(name) => values
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownVariable(name.clone())),
        Add(a, b) => Ok(Value::I64(
            as_i64(eval(a, values)?)?
                .checked_add(as_i64(eval(b, values)?)?)
                .ok_or(EvalError::Overflow)?,
        )),
        Sub(a, b) => Ok(Value::I64(
            as_i64(eval(a, values)?)?
                .checked_sub(as_i64(eval(b, values)?)?)
                .ok_or(EvalError::Overflow)?,
        )),
        Eq(a, b) => {
            let left = eval(a, values)?;
            let right = eval(b, values)?;
            if std::mem::discriminant(&left) != std::mem::discriminant(&right) {
                return Err(EvalError::SortMismatch);
            }
            Ok(Value::Bool(left == right))
        }
        Lt(a, b) => Ok(Value::Bool(
            as_i64(eval(a, values)?)? < as_i64(eval(b, values)?)?,
        )),
        Le(a, b) => Ok(Value::Bool(
            as_i64(eval(a, values)?)? <= as_i64(eval(b, values)?)?,
        )),
        Gt(a, b) => Ok(Value::Bool(
            as_i64(eval(a, values)?)? > as_i64(eval(b, values)?)?,
        )),
        Ge(a, b) => Ok(Value::Bool(
            as_i64(eval(a, values)?)? >= as_i64(eval(b, values)?)?,
        )),
        And(a, b) => Ok(Value::Bool(
            as_bool(eval(a, values)?)? & as_bool(eval(b, values)?)?,
        )),
        Or(a, b) => Ok(Value::Bool(
            as_bool(eval(a, values)?)? | as_bool(eval(b, values)?)?,
        )),
        Not(a) => Ok(Value::Bool(!as_bool(eval(a, values)?)?)),
        Uninterpreted { name, .. } => Err(EvalError::Unsupported(name.clone())),
    }
}

fn as_i64(value: Value) -> Result<i64, EvalError> {
    match value {
        Value::I64(v) => Ok(v),
        _ => Err(EvalError::SortMismatch),
    }
}

fn as_bool(value: Value) -> Result<bool, EvalError> {
    match value {
        Value::Bool(v) => Ok(v),
        _ => Err(EvalError::SortMismatch),
    }
}
