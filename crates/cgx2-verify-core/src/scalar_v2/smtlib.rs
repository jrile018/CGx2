use super::{Contract, Expr, Failure, Sort, ValidationError};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

pub const SMT_NODE_LIMIT: usize = 4096;
pub const SMT_DEPTH_LIMIT: usize = 128;
pub const SMT_OUTPUT_LIMIT: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmtLimits {
    pub nodes: usize,
    pub depth: usize,
    pub output_bytes: usize,
}
impl Default for SmtLimits {
    fn default() -> Self {
        Self {
            nodes: SMT_NODE_LIMIT,
            depth: SMT_DEPTH_LIMIT,
            output_bytes: SMT_OUTPUT_LIMIT,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmtError {
    InvalidContract(ValidationError),
    UnmodeledLocalBinding,
    InvalidLimits,
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },
}
/// Iterative shape check before recursive validation, sorting, or text allocation.
/// Identifier bytes are charged before hex encoding or copying names.
pub fn check_smt_limits(contract: &Contract, limits: SmtLimits) -> Result<(), SmtError> {
    if limits.nodes == 0
        || limits.nodes > SMT_NODE_LIMIT
        || limits.depth == 0
        || limits.depth > SMT_DEPTH_LIMIT
        || limits.output_bytes == 0
        || limits.output_bytes > SMT_OUTPUT_LIMIT
    {
        return Err(SmtError::InvalidLimits);
    }
    let limit = |resource, limit| SmtError::LimitExceeded { resource, limit };
    if contract.parameters.len() > limits.nodes {
        return Err(limit("parameters", limits.nodes));
    }
    let mut names = 0usize;
    let mut charge = |name: &str| -> Result<(), SmtError> {
        names = names
            .checked_add(name.len())
            .filter(|n| *n <= limits.output_bytes / 2)
            .ok_or_else(|| limit("identifier_bytes", limits.output_bytes / 2))?;
        Ok(())
    };
    for parameter in &contract.parameters {
        charge(&parameter.name)?;
    }
    let mut stack = vec![(&contract.requires, 1usize), (&contract.ensures, 1)];
    let mut nodes = contract.parameters.len();
    while let Some((expr, depth)) = stack.pop() {
        nodes += 1;
        if nodes > limits.nodes {
            return Err(limit("nodes", limits.nodes));
        }
        if depth > limits.depth {
            return Err(limit("depth", limits.depth));
        }
        use Expr::*;
        match expr {
            Var(name) => charge(name)?,
            Let(name, a, b) => {
                charge(name)?;
                stack.extend([(a.as_ref(), depth + 1), (b.as_ref(), depth + 1)]);
            }
            Add(a, b)
            | Sub(a, b)
            | Mul(a, b)
            | Div(a, b)
            | Eq(a, b)
            | Lt(a, b)
            | Le(a, b)
            | Gt(a, b)
            | Ge(a, b)
            | And(a, b)
            | Or(a, b) => {
                stack.extend([(a.as_ref(), depth + 1), (b.as_ref(), depth + 1)]);
            }
            Not(a) => stack.push((a, depth + 1)),
            If(g, a, b) => stack.extend([
                (g.as_ref(), depth + 1),
                (a.as_ref(), depth + 1),
                (b.as_ref(), depth + 1),
            ]),
            Uninterpreted { name, args, .. } => {
                charge(name)?;
                if args.len() > limits.nodes - nodes {
                    return Err(limit("nodes", limits.nodes));
                }
                stack.extend(args.iter().map(|arg| (arg, depth + 1)));
            }
            Bool(_) | I64(_) => {}
        }
        if stack.len() > limits.nodes - nodes {
            return Err(limit("nodes", limits.nodes));
        }
    }
    Ok(())
}
/// Every fragment is checked before String can reserve or copy it.
struct Text {
    value: String,
    limit: usize,
}
impl Text {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
        }
    }
    fn add(&mut self, args: fmt::Arguments<'_>) -> Result<(), SmtError> {
        self.write_fmt(args).map_err(|_| SmtError::LimitExceeded {
            resource: "output_bytes",
            limit: self.limit,
        })
    }
}
impl Write for Text {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.limit - self.value.len() {
            return Err(fmt::Error);
        }
        self.value.push_str(value);
        Ok(())
    }
}
#[derive(Clone)]
struct Encoded {
    term: String,
    defined: String,
    sort: Sort,
}
struct Encoder {
    text: Text,
    functions: BTreeMap<String, (Vec<Sort>, Sort)>,
    foreign_bounds: BTreeSet<String>,
    next: usize,
}
impl Encoder {
    fn bind(&mut self, term: &str, defined: &str, sort: Sort) -> Result<Encoded, SmtError> {
        let id = self.next;
        self.next += 1;
        let t = format!("cgx2_t_{id}");
        let d = format!("cgx2_d_{id}");
        self.text.add(format_args!(
            "(define-fun {t} () {} {term})\n(define-fun {d} () Bool {defined})\n",
            sort_name(sort)
        ))?;
        Ok(Encoded {
            term: t,
            defined: d,
            sort,
        })
    }
    fn encode<'a>(
        &mut self,
        expr: &'a Expr,
        variables: &BTreeMap<&'a str, Encoded>,
    ) -> Result<Encoded, SmtError> {
        use Expr::*;
        let (term, defined, sort) = match expr {
            Bool(v) => (v.to_string(), "true".into(), Sort::Bool),
            I64(v) => (int(*v), "true".into(), Sort::I64),
            Var(name) => {
                return variables.get(name.as_str()).cloned().ok_or_else(|| {
                    SmtError::InvalidContract(ValidationError::UnknownVariable(name.clone()))
                })
            }
            Let(name, value, body) => {
                let binding = self.encode(value, variables)?;
                let mut scope = variables.clone();
                scope.insert(name, binding.clone());
                let body = self.encode(body, &scope)?;
                (
                    body.term,
                    format!("(and {} {})", binding.defined, body.defined),
                    body.sort,
                )
            }
            If(g, a, b) => {
                let g = self.encode(g, variables)?;
                let a = self.encode(a, variables)?;
                let b = self.encode(b, variables)?;
                (
                    format!("(ite {} {} {})", g.term, a.term, b.term),
                    format!(
                        "(and {} (ite {} {} {}))",
                        g.defined, g.term, a.defined, b.defined
                    ),
                    a.sort,
                )
            }
            Not(a) => {
                let a = self.encode(a, variables)?;
                (format!("(not {})", a.term), a.defined, Sort::Bool)
            }
            Add(a, b) | Sub(a, b) | Mul(a, b) | Div(a, b) => {
                let a = self.encode(a, variables)?;
                let b = self.encode(b, variables)?;
                let raw = if matches!(expr, Div(..)) {
                    // Positive SMT div plus sign correction truncates toward zero.
                    let abs_a = format!("(ite (< {} 0) (- {}) {})", a.term, a.term, a.term);
                    let abs_b = format!("(ite (< {} 0) (- {}) {})", b.term, b.term, b.term);
                    let mag = format!("(div {abs_a} {abs_b})");
                    format!(
                        "(ite (= {} 0) 0 (ite (= (< {} 0) (< {} 0)) {mag} (- {mag})))",
                        b.term, a.term, b.term
                    )
                } else {
                    let op = match expr {
                        Add(..) => "+",
                        Sub(..) => "-",
                        _ => "*",
                    };
                    format!("({op} {} {})", a.term, b.term)
                };
                // Invalid children have no source value. Totalize their parents
                // to zero, bounding inactive multiplication chains in the audit.
                // Definedness still records every eager arithmetic failure.
                let inputs = format!("(and {} {})", a.defined, b.defined);
                let term = format!("(ite {inputs} {raw} 0)");
                let value = self.bind(&term, "true", Sort::I64)?;
                let nonzero = if matches!(expr, Div(..)) {
                    format!("(not (= {} 0))", b.term)
                } else {
                    "true".into()
                };
                let defined = format!("(and {inputs} {nonzero} (>= {} (- 9223372036854775808)) (<= {} 9223372036854775807))", value.term, value.term);
                return self.bind(&value.term, &defined, Sort::I64);
            }
            Eq(a, b) | Lt(a, b) | Le(a, b) | Gt(a, b) | Ge(a, b) | And(a, b) | Or(a, b) => {
                let a = self.encode(a, variables)?;
                let b = self.encode(b, variables)?;
                let op = match expr {
                    Eq(..) => "=",
                    Lt(..) => "<",
                    Le(..) => "<=",
                    Gt(..) => ">",
                    Ge(..) => ">=",
                    And(..) => "and",
                    _ => "or",
                };
                (
                    format!("({op} {} {})", a.term, b.term),
                    format!("(and {} {})", a.defined, b.defined),
                    Sort::Bool,
                )
            }
            Uninterpreted { name, sort, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.encode(arg, variables))
                    .collect::<Result<Vec<_>, _>>()?;
                let signatures = args.iter().map(|a| a.sort).collect::<Vec<_>>();
                let name = bounded_symbol(
                    "u",
                    &format!("{name}:{signatures:?}:{sort:?}"),
                    self.text.limit,
                )?;
                self.functions.insert(name.clone(), (signatures, *sort));
                let mut term = Text::new(self.text.limit);
                if args.is_empty() {
                    term.add(format_args!("{name}"))?;
                } else {
                    term.add(format_args!("({name}"))?;
                    for arg in &args {
                        term.add(format_args!(" {}", arg.term))?;
                    }
                    term.add(format_args!(")"))?;
                }
                let mut defined = Text::new(self.text.limit);
                defined.add(format_args!("(and true"))?;
                for arg in &args {
                    defined.add(format_args!(" {}", arg.defined))?;
                }
                defined.add(format_args!(")"))?;
                let value = self.bind(&term.value, &defined.value, *sort)?;
                if *sort == Sort::I64 {
                    self.foreign_bounds.insert(format!(
                        "(and (>= {} (- 9223372036854775808)) (<= {} 9223372036854775807))",
                        value.term, value.term
                    ));
                }
                return Ok(value);
            }
        };
        self.bind(&term, &defined, sort)
    }
}
fn context(contract: &Contract, limits: SmtLimits) -> Result<(Text, Encoded, Encoded), SmtError> {
    check_smt_limits(contract, limits)?;
    contract.validate().map_err(SmtError::InvalidContract)?;
    // Preserve conservative policy for global UF range assumptions.
    if (contract.requires.contains_binding() || contract.ensures.contains_binding())
        && (contract.requires.contains_uninterpreted() || contract.ensures.contains_uninterpreted())
    {
        return Err(SmtError::UnmodeledLocalBinding);
    }
    let mut encoder = Encoder {
        text: Text::new(limits.output_bytes),
        functions: BTreeMap::new(),
        foreign_bounds: BTreeSet::new(),
        next: 0,
    };
    let mut variables = BTreeMap::new();
    for p in &contract.parameters {
        let name = bounded_symbol("v", &p.name, limits.output_bytes)?;
        variables.insert(p.name.as_str(), encoder.bind(&name, "true", p.sort)?);
    }
    let requires = encoder.encode(&contract.requires, &variables)?;
    let ensures = encoder.encode(&contract.ensures, &variables)?;
    let mut text = Text::new(limits.output_bytes);
    text.add(format_args!("(set-option :produce-models true)\n(set-logic QF_UFNIA)\n; Checked i64; pure total uninterpreted external results are assumptions.\n"))?;
    for p in &contract.parameters {
        let name = bounded_symbol("v", &p.name, limits.output_bytes)?;
        text.add(format_args!(
            "(declare-const {name} {})\n",
            sort_name(p.sort)
        ))?;
        if p.sort == Sort::I64 {
            text.add(format_args!("(assert (>= {name} (- 9223372036854775808)))\n(assert (<= {name} 9223372036854775807))\n"))?;
        }
    }
    for (name, (args, sort)) in encoder.functions {
        text.add(format_args!("(declare-fun {name} ("))?;
        for (i, arg) in args.iter().enumerate() {
            text.add(format_args!(
                "{}{}",
                if i == 0 { "" } else { " " },
                sort_name(*arg)
            ))?;
        }
        text.add(format_args!(") {})\n", sort_name(sort)))?;
    }
    text.add(format_args!("{}", encoder.text.value))?;
    for bound in encoder.foreign_bounds {
        text.add(format_args!("(assert {bound})\n"))?;
    }
    Ok((text, requires, ensures))
}
pub fn emit_smtlib(contract: &Contract) -> Result<String, SmtError> {
    emit_smtlib_with_limits(contract, SmtLimits::default())
}
pub fn emit_smtlib_with_limits(contract: &Contract, limits: SmtLimits) -> Result<String, SmtError> {
    let (mut text, r, e) = context(contract, limits)?;
    text.add(format_args!(
        "; vc:requires_defined\n(push 1)\n(assert (not {}))\n(check-sat)\n(pop 1)\n",
        r.defined
    ))?;
    text.add(format_args!(
        "; vc:requires_satisfiable\n(push 1)\n(assert (and {} {}))\n(check-sat)\n(pop 1)\n",
        r.defined, r.term
    ))?;
    text.add(format_args!(
        "; vc:ensures_false\n(push 1)\n(assert (and {} {} {} (not {})))\n(check-sat)\n(pop 1)\n",
        r.defined, r.term, e.defined, e.term
    ))?;
    text.add(format_args!(
        "; vc:ensures_overflow\n(push 1)\n(assert (and {} {} (not {})))\n(check-sat)\n(pop 1)\n",
        r.defined, r.term, e.defined
    ))?;
    Ok(text.value)
}
pub fn emit_witness_smtlib(contract: &Contract, failure: Failure) -> Result<String, SmtError> {
    emit_witness_smtlib_with_limits(contract, failure, SmtLimits::default())
}
pub fn emit_witness_smtlib_with_limits(
    contract: &Contract,
    failure: Failure,
    limits: SmtLimits,
) -> Result<String, SmtError> {
    let (mut text, r, e) = context(contract, limits)?;
    let violation = match failure {
        Failure::EnsuresFalse => format!("(and {} (not {}))", e.defined, e.term),
        Failure::Overflow => format!("(not {})", e.defined),
    };
    text.add(format_args!(
        "(assert (and {} {} {violation}))\n(check-sat)\n",
        r.defined, r.term
    ))?;
    if !contract.parameters.is_empty() {
        text.add(format_args!("(get-value ("))?;
        for (i, p) in contract.parameters.iter().enumerate() {
            text.add(format_args!(
                "{}{}",
                if i == 0 { "" } else { " " },
                bounded_symbol("v", &p.name, limits.output_bytes)?
            ))?;
        }
        text.add(format_args!("))\n"))?;
    }
    Ok(text.value)
}
fn sort_name(sort: Sort) -> &'static str {
    match sort {
        Sort::Bool => "Bool",
        Sort::I64 => "Int",
    }
}
fn bounded_symbol(prefix: &str, name: &str, limit: usize) -> Result<String, SmtError> {
    let mut text = Text::new(limit);
    text.add(format_args!("{prefix}_"))?;
    for byte in name.bytes() {
        text.add(format_args!("{byte:02x}"))?;
    }
    Ok(text.value)
}
pub(crate) fn symbol(prefix: &str, name: &str) -> String {
    let mut result = String::from(prefix);
    result.push('_');
    for byte in name.bytes() {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}
fn int(value: i64) -> String {
    if value < 0 {
        format!("(- {})", -(value as i128))
    } else {
        value.to_string()
    }
}
