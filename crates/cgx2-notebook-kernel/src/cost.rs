//! Static bounds for the checked interpreter, including generated value replay.
use std::collections::{BTreeMap, BTreeSet};

use cgx2_syntax::{parse_module, Expr, ExprKind, Function, Module, Stmt, StmtKind};

use super::{literal_range_trips, MAX_PURE_FUNCTIONS, MAX_REPLAY_BYTES, MAX_STATIC_STEPS};

#[derive(Clone, Copy, Default)]
struct Cost {
    steps: usize,
    depth: usize,
}

impl Cost {
    fn steps(steps: usize) -> Self {
        Self { steps, depth: 0 }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            steps: self.steps.saturating_add(other.steps),
            depth: self.depth.max(other.depth),
        }
    }
}

struct Model<'a> {
    functions: BTreeMap<&'a str, &'a Function>,
    records: BTreeSet<&'a str>,
    completed: BTreeMap<String, Cost>,
    stack: BTreeSet<String>,
}

impl Model<'_> {
    fn function(&mut self, name: &str) -> Result<Cost, String> {
        if let Some(cost) = self.completed.get(name) {
            return Ok(*cost);
        }
        if !self.stack.insert(name.to_owned()) {
            return Err(format!(
                "CGNK011: recursive pure function {name} is unsupported"
            ));
        }
        if self.stack.len() > 32 {
            return Err("CGNK006: pure function call depth exceeded".into());
        }
        let function = *self.functions.get(name).ok_or_else(|| format!("CGNK011: unresolved call {name}; define or rerun a pure function before this cell; file, network, and foreign effects cannot run"))?;
        let mut cost = self.block(&function.body)?;
        // The interpreter evaluates each contract once per call, including calls
        // reached only from contracts. Traverse all of them for recursion/depth
        // and unresolved/effectful calls even when a previous condition is false.
        for contract in function.requires.iter().chain(&function.ensures) {
            cost = cost.plus(self.expression(contract)?);
        }
        cost.depth = cost.depth.saturating_add(1);
        if cost.depth > 32 {
            return Err("CGNK006: pure function call depth exceeded".into());
        }
        self.stack.remove(name);
        self.completed.insert(name.to_owned(), cost);
        Ok(cost)
    }

    fn block(&mut self, body: &[Stmt]) -> Result<Cost, String> {
        let mut cost = Cost::default();
        // Do not stop at saturation: unreachable/empty bodies must still resolve calls.
        for statement in body {
            cost = cost.plus(self.statement(statement)?);
        }
        Ok(cost)
    }

    fn statement(&mut self, statement: &Stmt) -> Result<Cost, String> {
        let cost = match &statement.kind {
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => self
                .expression(condition)?
                .plus(self.block(then_body)?)
                .plus(self.block(else_body)?),
            StmtKind::For { iter, body, .. } => {
                let trips = literal_range_trips(iter).map_err(|error| error.to_string())?;
                let body = self.block(body)?;
                // One iterator step and two literal evaluations, plus one step per iteration.
                // Traverse even zero-trip bodies for names, recursion and depth checks.
                Cost {
                    steps: 3usize
                        .saturating_add(trips.saturating_mul(1usize.saturating_add(body.steps))),
                    depth: body.depth,
                }
            }
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value)
            | StmtKind::Return(Some(value)) => self.expression(value)?,
            StmtKind::Return(None) => Cost::default(),
            _ => return Err("CGNK011: unsupported statement in pure notebook cell".into()),
        };
        Ok(Cost::steps(1).plus(cost))
    }

    fn expression(&mut self, expression: &Expr) -> Result<Cost, String> {
        let cost = match &expression.kind {
            ExprKind::Constructor { .. } | ExprKind::Collection(_) | ExprKind::Index { .. } => {
                return Err(format!(
                    "CGNK026: S2a expression cost/execution requires S3-N at {}..{}",
                    expression.span.start, expression.span.end
                ))
            }
            ExprKind::Name(_)
            | ExprKind::Integer(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Boolean(_) => Cost::default(),
            ExprKind::Binary { left, right, .. } => {
                self.expression(left)?.plus(self.expression(right)?)
            }
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(path) = &callee.kind else {
                    return Err("CGNK011: computed calls are unsupported".into());
                };
                let name = super::simple_name(path).map_err(|error| error.to_string())?;
                let mut cost = Cost::default();
                for argument in arguments {
                    cost = cost.plus(self.expression(&argument.value)?);
                }
                if !matches!(
                    name,
                    "$cgx2_cmp_eq"
                        | "$cgx2_cmp_ne"
                        | "$cgx2_cmp_lt"
                        | "$cgx2_cmp_le"
                        | "$cgx2_cmp_gt"
                        | "$cgx2_cmp_ge"
                ) && !self.records.contains(name)
                {
                    cost = cost.plus(self.function(name)?);
                }
                cost
            }
        };
        Ok(Cost::steps(1).plus(cost))
    }
}

pub(super) fn validate_static_bounds(
    function_source: &str,
    module: &Module,
    replay_prefix: &str,
) -> Result<(), String> {
    let functions = parse_module(function_source).map_err(|diagnostics| {
        let first = &diagnostics[0];
        format!("{}: {}", first.code, first.message)
    })?;
    if functions.functions.len() > MAX_PURE_FUNCTIONS {
        return Err(format!("CGNK006: combined retained source has {} pure functions and {} bytes; limits are {MAX_PURE_FUNCTIONS} functions and {MAX_REPLAY_BYTES} bytes", functions.functions.len(), function_source.len()));
    }
    let mut model = Model {
        functions: BTreeMap::new(),
        records: functions
            .records
            .iter()
            .map(|record| record.name.as_str())
            .collect(),
        completed: BTreeMap::new(),
        stack: BTreeSet::new(),
    };
    for function in &functions.functions {
        if model.functions.insert(&function.name, function).is_some() {
            return Err(format!(
                "CGNK011: duplicate pure function {}",
                function.name
            ));
        }
    }
    for name in model.functions.keys().copied().collect::<Vec<_>>() {
        if model.function(name)?.steps > MAX_STATIC_STEPS {
            return Err("CGNK006: pure function step budget exceeded".into());
        }
    }
    let prefix = parse_module(replay_prefix).map_err(|_| "CGNK010: invalid replay prefix")?;
    let prefix_cost = model.block(&prefix.statements)?;
    let mut total = Cost::default();
    let mut pending = 0usize;
    for statement in &module.statements {
        total = total.plus(prefix_cost).plus(model.statement(statement)?);
        // A replayed binding costs at most 42 steps: let + record constructor +
        // eight scalar fields, each serialized MIN i64 costing five expressions.
        // Scalars cost at most six. Pending values are unknown until execution;
        // this covers every checked scalar/record result without running it.
        total = total.plus(Cost::steps(pending.saturating_mul(42)));
        if matches!(&statement.kind, StmtKind::Let { .. }) {
            pending = pending.saturating_add(1);
            // Generated trailing expression retrieving the newly bound value.
            total = total.plus(Cost::steps(2));
        }
    }
    if total.steps > MAX_STATIC_STEPS {
        return Err("CGNK006: cell step budget exceeded (including generated replay)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(functions: &str, cell: &str, prefix: &str) -> Result<(), String> {
        validate_static_bounds(functions, &parse_module(cell).unwrap(), prefix)
    }

    #[test]
    fn exact_step_boundary_includes_invocation_and_generated_result() {
        // Function body = 2 + 4 + 33330 * (1 + 2) + 2 = 99,998.
        let f = "fn f() -> i64:\n    let mut n = 0\n    for i in range(0, 33330):\n        n += 1\n    n\n";
        // Exactly 100,000; an extra statement, generated retrieval or MIN replay crosses it.
        assert!(check(f, "f()", "").is_ok());
        let one_less_iteration = f.replace("33330", "33329");
        assert!(check(&one_less_iteration, "f()\n0", "").is_ok()); // 99,999
        assert!(check(&one_less_iteration, "f()\n0\n0", "") // 100,001
            .unwrap_err()
            .starts_with("CGNK006"));
        for (cell, prefix) in [
            ("f()\n0", ""),
            ("let n = f()", ""),
            ("f()", "let n = 0 - 9223372036854775807 - 1\n"),
        ] {
            assert!(check(f, cell, prefix).unwrap_err().starts_with("CGNK006"));
        }
        assert!(check(&f.replace("33330", "33331"), "", "")
            .unwrap_err()
            .starts_with("CGNK006"));
    }

    #[test]
    fn nested_costs_count_setup_iteration_branches_and_callees() {
        let source = "fn pure() -> i64:\n    1\nfn f() -> i64:\n    for i in range(0, 2):\n        for j in range(0, 3):\n            if true:\n                pure()\n            else:\n                0\n    0\n";
        let syntax = parse_module(source).unwrap();
        let mut model = Model {
            functions: syntax
                .functions
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect(),
            records: BTreeSet::new(),
            completed: BTreeMap::new(),
            stack: BTreeSet::new(),
        };
        // pure = 2; if = 1+1 + (1+1+2) + (1+1) = 8;
        // inner = 4+3*(1+8) = 31; outer = 4+2*(1+31) = 68; tail = 2.
        assert_eq!(model.function("f").unwrap().steps, 70);
        assert_eq!(model.function("f").unwrap().depth, 2);
    }

    #[test]
    fn contracts_charge_each_invocation_and_replayed_binding_at_exact_boundary() {
        let f = "fn f() -> i64:\n    requires true\n    ensures true\n    let mut n = 0\n    for i in range(0, 33329):\n        n += 1\n    n\n";
        // Body 99,995 + two conditions = 99,997. Call/statement = two.
        assert!(check(f, "f()", "").is_ok()); // 99,999
        let exact = f.replace("requires true", "requires true\n    requires true");
        assert!(check(&exact, "f()", "").is_ok()); // 100,000
        let over = exact.replace("ensures true", "ensures true\n    ensures true");
        assert!(check(&over, "f()", "").unwrap_err().starts_with("CGNK006"));
        for (cell, prefix) in [("f()\n0", ""), ("let n = f()", ""), ("f()", "let n = 0\n")] {
            assert!(check(f, cell, prefix).unwrap_err().starts_with("CGNK006"));
        }
        // A helper used exclusively by a condition contributes its full loop cost.
        let helper = "fn guard() -> Bool:\n    for i in range(0, 33333):\n        0\n    true\nfn f() -> i64:\n    requires guard()\n    0\n";
        assert!(check(helper, "", "").unwrap_err().starts_with("CGNK006"));
    }

    #[test]
    fn contract_calls_resolve_even_when_unreached_and_include_cached_depth() {
        for conditions in ["requires f() == 0", "requires false\n    ensures unknown()"] {
            let f = format!("fn f() -> i64:\n    {conditions}\n    0\n");
            assert!(check(&f, "", "").unwrap_err().starts_with("CGNK011"));
        }
        let mut source = "fn f0() -> Bool:\n    true\n".to_owned();
        for i in 1..32 {
            source.push_str(&format!(
                "fn f{i}() -> Bool:\n    requires f{}()\n    true\n",
                i - 1
            ));
        }
        assert!(check(&source, "", "").is_ok());
        source.push_str("fn f32() -> Bool:\n    ensures f31()\n    true\n");
        // This tests cost/depth alone, not the independently enforced 32-function count.
        let syntax = parse_module(&source).unwrap();
        let mut model = Model {
            functions: syntax
                .functions
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect(),
            records: BTreeSet::new(),
            completed: BTreeMap::new(),
            stack: BTreeSet::new(),
        };
        assert!(model.function("f31").is_ok());
        assert!(model.function("f32").err().unwrap().starts_with("CGNK006"));
    }

    #[test]
    fn empty_bodies_still_resolve_calls_and_check_recursion_and_depth() {
        for call in ["unknown()", "f()"] {
            let f = format!("fn f() -> i64:\n    for i in range(0, 0):\n        {call}\n    0\n");
            assert!(check(&f, "", "").unwrap_err().starts_with("CGNK011"));
        }
        // Cache entries must retain depth even when populated dependency-first.
        let mut f = "fn f0() -> i64:\n    0\n".to_owned();
        for i in 1..32 {
            f.push_str(&format!("fn f{i}() -> i64:\n    f{}()\n", i - 1));
        }
        assert!(check(&f, "", "").is_ok());
        f.push_str("fn f32() -> i64:\n    f31()\n");
        assert!(check(&f, "", "").unwrap_err().starts_with("CGNK006"));
    }

    #[test]
    fn pending_and_retained_record_replay_are_charged_for_every_invocation() {
        let record = "record P:\n    a: i64\n    b: i64\n    c: i64\n    d: i64\n    e: i64\n    f: i64\n    g: i64\n    h: i64\n";
        let mut model = Model {
            functions: BTreeMap::new(),
            records: BTreeSet::from(["P"]),
            completed: BTreeMap::new(),
            stack: BTreeSet::new(),
        };
        let value = "0 - 9223372036854775807 - 1";
        let fields = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .map(|field| format!("{field}: {value}"))
            .join(", ");
        let prefix = format!("let p = P({fields})\n");
        assert_eq!(
            model
                .block(&parse_module(&prefix).unwrap().statements)
                .unwrap()
                .steps,
            42
        );
        let cell = "0\n".repeat(2272); // 2272*(42+2) = 99,968
        assert!(check(record, &cell, &prefix).is_ok());
        assert!(check(record, &(cell + "0\n"), &prefix)
            .unwrap_err()
            .starts_with("CGNK006"));
        let pending = (0..100)
            .map(|i| format!("let n{i} = 0\n"))
            .collect::<String>();
        assert!(check("", &pending, "").unwrap_err().starts_with("CGNK006"));
    }
}
