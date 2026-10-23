use cgx2_sema::{check_module, TypedModule};
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Stmt, StmtKind, TypeSyntax};
use std::collections::{BTreeMap, BTreeSet};

use crate::error::EmitDiagnostic;
use crate::names::python_name;

const PRELUDE: &str = r#"# Generated from CGx2's checked scalar profile.
import builtins as _cgx2_builtins
import sys as _cgx2_sys

_cgx2_min = -9223372036854775808
_cgx2_max = 9223372036854775807
_cgx2_steps = 0
_cgx2_depth = 0

def _cgx2_step(start, end):
    global _cgx2_steps
    if _cgx2_steps >= 1000000:
        raise _cgx2_builtins.RuntimeError(f"CGR010 at {start}..{end}: execution step limit of 1000000 exceeded")
    _cgx2_steps += 1

def _cgx2_enter(start, end):
    global _cgx2_depth
    if _cgx2_depth >= 64:
        raise _cgx2_builtins.RuntimeError(f"CGR011 at {start}..{end}: function call depth limit of 64 exceeded")
    _cgx2_depth += 1

def _cgx2_leave():
    global _cgx2_depth
    _cgx2_depth -= 1


def _cgx2_i64(value):
    if _cgx2_builtins.type(value) is not _cgx2_builtins.int or not (_cgx2_min <= value <= _cgx2_max):
        raise _cgx2_builtins.OverflowError("CGR005: i64 arithmetic overflow")
    return value

def _cgx2_add(left, right):
    return _cgx2_i64(left + right)

def _cgx2_sub(left, right):
    return _cgx2_i64(left - right)

def _cgx2_mul(left, right):
    return _cgx2_i64(left * right)

def _cgx2_div(left, right):
    if right == 0:
        raise _cgx2_builtins.ZeroDivisionError("CGR006: division by zero")
    quotient = _cgx2_builtins.abs(left) // _cgx2_builtins.abs(right)
    if (left < 0) != (right < 0):
        quotient = -quotient
    return _cgx2_i64(quotient)

def _cgx2_arg_count():
    return _cgx2_i64(_cgx2_builtins.len(_cgx2_sys.argv) - 1)

def _cgx2_arg(index):
    if index < 0 or index >= _cgx2_arg_count():
        raise _cgx2_builtins.IndexError("arg index out of range")
    return _cgx2_sys.argv[index + 1]

def _cgx2_show(value):
    if _cgx2_builtins.type(value) is _cgx2_builtins.bool:
        return "true" if value else "false"
    if _cgx2_builtins.type(value) is _cgx2_builtins.int:
        return _cgx2_builtins.str(_cgx2_i64(value))
    return value

class _cgx2_ContractError(_cgx2_builtins.RuntimeError):
    def __init__(self, code, start, end):
        self.code = code
        self.span = (start, end)
        super().__init__(f"{code}: contract condition failed at bytes [{start}, {end})")

def _cgx2_contract_check(value, code, start, end):
    if value is not True:
        raise _cgx2_ContractError(code, start, end)

"#;

const PACKAGE_HELPERS: &str = "# Generated from a checked CGx2 module.\nfrom _cgx2_support import _cgx2_add, _cgx2_sub, _cgx2_mul, _cgx2_div\nfrom _cgx2_support import _cgx2_arg, _cgx2_arg_count, _cgx2_show, _cgx2_contract_check\nfrom _cgx2_support import _cgx2_step, _cgx2_enter, _cgx2_leave\n\n";

pub(crate) fn package_support() -> &'static str {
    PRELUDE
}

pub(crate) fn emit(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    emit_impl(module, typed, false, "")
}

pub(crate) fn emit_package_module(
    module: &Module,
    typed: &TypedModule,
    imports: &str,
) -> Result<String, Vec<EmitDiagnostic>> {
    emit_impl(module, typed, true, imports)
}

fn emit_impl(
    module: &Module,
    typed: &TypedModule,
    package_mode: bool,
    imports: &str,
) -> Result<String, Vec<EmitDiagnostic>> {
    let mut errors = Vec::new();
    for import in &module.imports {
        errors.push(EmitDiagnostic::new(
            "CGE001",
            "imports are unsupported by Python emission",
            import.span,
        ));
    }
    if !module.functions.is_empty() && !module.statements.is_empty() {
        errors.push(EmitDiagnostic::new(
            "CGE001",
            "function items and script statements cannot be mixed",
            module.statements[0].span,
        ));
    }
    for function in &module.functions {
        if let Some(binding) = &function.foreign {
            errors.push(EmitDiagnostic::new(
                "CGE001",
                "foreign declarations are unsupported by Python source emission",
                binding.target.span,
            ));
        }
        for parameter in &function.parameters {
            validate_type(&parameter.ty, &mut errors);
        }
        if let Some(ty) = &function.return_type {
            validate_type(ty, &mut errors);
        }
        for contract in function.requires.iter().chain(&function.ensures) {
            validate_expr(contract, &mut errors);
        }
        validate_block(&function.body, false, &mut errors);
    }
    validate_block(&module.statements, true, &mut errors);
    if !package_mode && !module.functions.is_empty() {
        match module
            .functions
            .iter()
            .find(|function| function.name == "main")
        {
            Some(main) if !main.parameters.is_empty() => errors.push(EmitDiagnostic::new(
                "CGE001",
                "main must have zero parameters",
                main.span,
            )),
            None => errors.push(EmitDiagnostic::new(
                "CGE001",
                "function module requires a main function",
                module.span,
            )),
            _ => {}
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if !package_mode {
        match check_module(module) {
            Ok(fresh) if fresh == *typed => {}
            _ => {
                return Err(vec![EmitDiagnostic::new(
                    "CGE002",
                    "typed summary does not match the syntax tree",
                    module.span,
                )]);
            }
        }
    }
    for function in &module.functions {
        validate_metadata(function, typed, package_mode, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut module = module.clone();
    scope_module(&mut module);
    let mut output = String::from(if package_mode {
        PACKAGE_HELPERS
    } else {
        PRELUDE
    });
    output.push_str(imports);
    if !imports.is_empty() {
        output.push('\n');
    }
    for function in &module.functions {
        let params = function
            .parameters
            .iter()
            .map(|parameter| python_name(&parameter.name))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("def {}({params}):\n", python_name(&function.name)));
        output.push_str(&format!(
            "    _cgx2_enter({}, {})\n",
            function.span.start, function.span.end
        ));
        output.push_str("    try:\n");
        if function.requires.is_empty() && function.ensures.is_empty() {
            emit_block(&function.body, 2, true, &mut output);
        } else {
            for condition in &function.requires {
                emit_contract_check(condition, "CGR015", 2, false, &mut output);
            }
            output.push_str("        def _cgx2_contract_body():\n");
            emit_block(&function.body, 3, true, &mut output);
            output.push_str("        _cgx2_contract_result = _cgx2_contract_body()\n");
            for condition in &function.ensures {
                emit_contract_check(condition, "CGR016", 2, true, &mut output);
            }
            output.push_str("        return _cgx2_contract_result\n");
        }
        output.push_str("    finally:\n        _cgx2_leave()\n");
        output.push('\n');
    }
    if module.functions.is_empty() {
        output.push_str("def _cgx2_script():\n");
        emit_block(&module.statements, 1, true, &mut output);
        output.push('\n');
    }
    if package_mode {
        return Ok(output);
    }
    let entry = if module.functions.is_empty() {
        "_cgx2_script()".to_owned()
    } else {
        format!("{}()", python_name("main"))
    };
    output.push_str("if __name__ == \"__main__\":\n");
    output.push_str(&format!("    _cgx2_result = {entry}\n"));
    output.push_str("    if _cgx2_result is not None:\n");
    output.push_str("        _cgx2_sys.stdout.write(_cgx2_show(_cgx2_result) + \"\\n\")\n");
    Ok(output)
}

fn validate_metadata(
    function: &cgx2_syntax::Function,
    typed: &TypedModule,
    package_mode: bool,
    errors: &mut Vec<EmitDiagnostic>,
) {
    if let Some(declaration) = &function.effects {
        for name in &declaration.names {
            if !matches!(name.name.as_str(), "Mutate" | "ReadArgs") {
                errors.push(EmitDiagnostic::new(
                    "CGE004",
                    format!("effect `{}` is unsupported by Python emission", name.name),
                    name.span,
                ));
            }
        }
    }
    if let Some(declaration) = &function.capabilities {
        for name in &declaration.names {
            if name.name != "ArgsRead" {
                errors.push(EmitDiagnostic::new(
                    "CGE004",
                    format!(
                        "capability `{}` is unsupported by Python emission",
                        name.name
                    ),
                    name.span,
                ));
            }
        }
    }
    if let Some(summary) = typed.effects.get(&function.name) {
        for effect in &summary.effects {
            if !matches!(effect.as_str(), "Mutate" | "ReadArgs")
                && !(package_mode && effect == "Unknown")
            {
                errors.push(EmitDiagnostic::new(
                    "CGE004",
                    format!("inferred effect `{effect}` is unsupported by Python emission"),
                    function.span,
                ));
            }
        }
        for capability in &summary.capabilities {
            if capability != "ArgsRead" {
                errors.push(EmitDiagnostic::new(
                    "CGE004",
                    format!("inferred capability `{capability}` is unsupported by Python emission"),
                    function.span,
                ));
            }
        }
    }
}

fn emit_contract_check(
    condition: &Expr,
    code: &str,
    indent: usize,
    bind_result: bool,
    output: &mut String,
) {
    output.push_str(&format!(
        "{}_cgx2_contract_check({}, \"{code}\", {}, {})\n",
        "    ".repeat(indent),
        emit_expr_with_result(condition, bind_result),
        condition.span.start,
        condition.span.end,
    ));
}

fn validate_type(ty: &TypeSyntax, errors: &mut Vec<EmitDiagnostic>) {
    if !ty.arguments.is_empty()
        || ty.path.segments.len() != 1
        || !matches!(
            ty.path.segments[0].as_str(),
            "i64" | "Bool" | "String" | "Unit"
        )
    {
        errors.push(EmitDiagnostic::new(
            "CGE001",
            format!("type {ty} is unsupported by Python emission"),
            ty.span,
        ));
    }
}

fn validate_block(stmts: &[Stmt], script: bool, errors: &mut Vec<EmitDiagnostic>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    validate_type(ty, errors);
                }
                validate_expr(value, errors);
            }
            StmtKind::Assign { target, value, .. } => {
                if target.segments.len() != 1 {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "qualified assignment is unsupported by Python emission",
                        target.span,
                    ));
                }
                validate_expr(value, errors);
            }
            StmtKind::Return(value) => {
                if script {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "return outside a function is unsupported",
                        stmt.span,
                    ));
                }
                if let Some(expr) = value {
                    validate_expr(expr, errors);
                }
            }
            StmtKind::Expr(expr) => validate_expr(expr, errors),
            StmtKind::For { iter, body, .. } => {
                let valid = matches!(&iter.kind, ExprKind::Call { callee, arguments }
                    if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                    && arguments.len() == 2 && arguments.iter().all(|arg| arg.name.is_none()));
                if valid {
                    validate_expr(iter, errors);
                    validate_block(body, script, errors);
                } else {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "only range(start, end) loops are supported",
                        stmt.span,
                    ));
                }
            }
            StmtKind::NoGc { .. } => errors.push(EmitDiagnostic::new(
                "CGE001",
                "memory constructs are unsupported by Python emission",
                stmt.span,
            )),
        }
    }
}

fn validate_expr(expr: &Expr, errors: &mut Vec<EmitDiagnostic>) {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() != 1 => errors.push(EmitDiagnostic::new(
            "CGE001",
            "qualified names are unsupported by Python emission",
            path.span,
        )),
        ExprKind::Integer(raw) if raw.replace('_', "").parse::<i64>().is_err() => {
            errors.push(EmitDiagnostic::new(
                "CGE003",
                "integer literal cannot be represented as i64",
                expr.span,
            ));
        }
        ExprKind::StringLiteral(raw) if decode_string(raw).is_none() => {
            errors.push(EmitDiagnostic::new(
                "CGE003",
                "string literal contains an unsupported escape",
                expr.span,
            ));
        }
        ExprKind::Binary { left, right, .. } => {
            validate_expr(left, errors);
            validate_expr(right, errors);
        }
        ExprKind::Call { callee, arguments } => {
            if matches!(&callee.kind, ExprKind::Name(path) if path.segments.first().is_some_and(|s| s == "memory"))
            {
                errors.push(EmitDiagnostic::new(
                    "CGE001",
                    "memory operations are unsupported by Python emission",
                    expr.span,
                ));
                return;
            }
            if !matches!(&callee.kind, ExprKind::Name(path) if path.segments.len() == 1) {
                errors.push(EmitDiagnostic::new(
                    "CGE001",
                    "computed or qualified calls are unsupported by Python emission",
                    callee.span,
                ));
            }
            for arg in arguments {
                if arg.name.is_some() {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "named calls are unsupported by Python emission",
                        arg.span,
                    ));
                }
                validate_expr(&arg.value, errors);
            }
        }
        _ => {}
    }
}

// CGx2 blocks have lexical bindings; Python function locals do not. Give every
// binding a stable name before writing source, while leaving function callees alone.
fn collect_bindings(block: &[Stmt], used: &mut BTreeSet<String>) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let { name, .. } => {
                used.insert(name.clone());
            }
            StmtKind::For { binding, body, .. } => {
                used.insert(binding.clone());
                collect_bindings(body, used);
            }
            StmtKind::NoGc { body, .. } => collect_bindings(body, used),
            _ => {}
        }
    }
}

fn scoped_name(
    original: &str,
    used: &mut BTreeSet<String>,
    assigned: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
    next: &mut usize,
) -> String {
    if !assigned.contains(original) && !reserved.contains(original) {
        assigned.insert(original.to_owned());
        return original.to_owned();
    }
    loop {
        let name = format!("cgx2_scope_{}", *next);
        *next += 1;
        if used.insert(name.clone()) {
            assigned.insert(name.clone());
            return name;
        }
    }
}

fn lookup_scoped(name: &str, scopes: &[BTreeMap<String, String>]) -> Option<String> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| scope.get(name).cloned())
}

fn scope_expr(expr: &mut Expr, scopes: &[BTreeMap<String, String>]) {
    match &mut expr.kind {
        ExprKind::Name(path) if path.segments.len() == 1 => {
            if let Some(name) = lookup_scoped(&path.segments[0], scopes) {
                path.segments[0] = name;
            }
        }
        ExprKind::Binary { left, right, .. } => {
            scope_expr(left, scopes);
            scope_expr(right, scopes);
        }
        ExprKind::Call { arguments, .. } => {
            for arg in arguments {
                scope_expr(&mut arg.value, scopes);
            }
        }
        _ => {}
    }
}

fn scope_block(
    block: &mut [Stmt],
    scopes: &mut Vec<BTreeMap<String, String>>,
    used: &mut BTreeSet<String>,
    assigned: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
    next: &mut usize,
) {
    for stmt in block {
        match &mut stmt.kind {
            StmtKind::Let { name, value, .. } => {
                scope_expr(value, scopes);
                let original = name.clone();
                *name = scoped_name(&original, used, assigned, reserved, next);
                scopes
                    .last_mut()
                    .expect("scope")
                    .insert(original, name.clone());
            }
            StmtKind::Assign { target, value, .. } => {
                scope_expr(value, scopes);
                if target.segments.len() == 1 {
                    if let Some(name) = lookup_scoped(&target.segments[0], scopes) {
                        target.segments[0] = name;
                    }
                }
            }
            StmtKind::Return(Some(value)) | StmtKind::Expr(value) => scope_expr(value, scopes),
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                scope_expr(iter, scopes);
                scopes.push(BTreeMap::new());
                let original = binding.clone();
                *binding = scoped_name(&original, used, assigned, reserved, next);
                scopes
                    .last_mut()
                    .expect("scope")
                    .insert(original, binding.clone());
                scope_block(body, scopes, used, assigned, reserved, next);
                scopes.pop();
            }
            StmtKind::NoGc { budget, body } => {
                scope_expr(budget, scopes);
                scopes.push(BTreeMap::new());
                scope_block(body, scopes, used, assigned, reserved, next);
                scopes.pop();
            }
            StmtKind::Return(None) => {}
        }
    }
}

fn scope_module(module: &mut Module) {
    let mut used = BTreeSet::new();
    let mut reserved = BTreeSet::from(["range".to_owned()]);
    for function in &module.functions {
        used.insert(function.name.clone());
        reserved.insert(function.name.clone());
        used.extend(function.parameters.iter().map(|p| p.name.clone()));
        collect_bindings(&function.body, &mut used);
    }
    collect_bindings(&module.statements, &mut used);
    let mut next = 0;
    for function in &mut module.functions {
        let mut scopes = vec![BTreeMap::new()];
        let mut assigned = BTreeSet::new();
        for parameter in &mut function.parameters {
            let original = parameter.name.clone();
            parameter.name = scoped_name(&original, &mut used, &mut assigned, &reserved, &mut next);
            scopes[0].insert(original, parameter.name.clone());
        }
        for condition in &mut function.requires {
            scope_expr(condition, &scopes);
        }
        scope_block(
            &mut function.body,
            &mut scopes,
            &mut used,
            &mut assigned,
            &reserved,
            &mut next,
        );
        // `result` is the interpreter's special contract binding, not a local.
        let mut contract_scopes = scopes.clone();
        for scope in &mut contract_scopes {
            scope.remove("result");
        }
        for condition in &mut function.ensures {
            scope_expr(condition, &contract_scopes);
        }
    }
    scope_block(
        &mut module.statements,
        &mut vec![BTreeMap::new()],
        &mut used,
        &mut BTreeSet::new(),
        &reserved,
        &mut next,
    );
}

fn emit_block(stmts: &[Stmt], indent: usize, return_tail: bool, output: &mut String) {
    for (index, stmt) in stmts.iter().enumerate() {
        output.push_str(&format!(
            "{}# CGx2 bytes [{}..{})\n",
            "    ".repeat(indent),
            stmt.span.start,
            stmt.span.end
        ));
        output.push_str(&format!(
            "{}_cgx2_step({}, {})\n",
            "    ".repeat(indent),
            stmt.span.start,
            stmt.span.end
        ));
        output.push_str(&"    ".repeat(indent));
        let last = index + 1 == stmts.len();
        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                output.push_str(&format!("{} = {}\n", python_name(name), emit_expr(value)));
            }
            StmtKind::Assign { target, op, value } => {
                let name = python_name(&target.segments[0]);
                let rhs = emit_expr(value);
                match op {
                    AssignOp::Set => output.push_str(&format!("{name} = {rhs}\n")),
                    AssignOp::Add => {
                        output.push_str(&format!("{name} = _cgx2_add({name}, {rhs})\n"));
                    }
                }
            }
            StmtKind::Return(value) => match value {
                Some(value) => output.push_str(&format!("return {}\n", emit_expr(value))),
                None => output.push_str("return None\n"),
            },
            StmtKind::Expr(expr) if last && return_tail => {
                output.push_str(&format!("return {}\n", emit_expr(expr)));
            }
            StmtKind::Expr(expr) => output.push_str(&format!("{}\n", emit_expr(expr))),
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                output.push_str(&format!(
                    "for {} in {}:\n",
                    python_name(binding),
                    emit_expr(iter)
                ));
                output.push_str(&format!(
                    "{}_cgx2_step({}, {})\n",
                    "    ".repeat(indent + 1),
                    stmt.span.start,
                    stmt.span.end
                ));
                emit_block(body, indent + 1, false, output);
            }
            StmtKind::NoGc { .. } => unreachable!("validated before emission"),
        }
    }
    if return_tail
        && (stmts.is_empty()
            || !matches!(
                stmts.last().map(|s| &s.kind),
                Some(StmtKind::Return(_) | StmtKind::Expr(_))
            ))
    {
        output.push_str(&"    ".repeat(indent));
        output.push_str("return None\n");
    }
}

fn emit_expr(expr: &Expr) -> String {
    emit_expr_with_result(expr, false)
}

fn emit_expr_with_result(expr: &Expr, bind_result: bool) -> String {
    let value = match &expr.kind {
        ExprKind::Name(path) if bind_result && path.segments[0] == "result" => {
            "_cgx2_contract_result".to_owned()
        }
        ExprKind::Name(path) => python_name(&path.segments[0]),
        ExprKind::Integer(raw) => raw.replace('_', ""),
        ExprKind::StringLiteral(raw) => {
            quote_python(&decode_string(raw).expect("validated string"))
        }
        ExprKind::Boolean(value) => if *value { "True" } else { "False" }.to_owned(),
        ExprKind::Binary { left, op, right } => {
            let helper = match op {
                BinaryOp::Add => "_cgx2_add",
                BinaryOp::Subtract => "_cgx2_sub",
                BinaryOp::Multiply => "_cgx2_mul",
                BinaryOp::Divide => "_cgx2_div",
            };
            format!(
                "{helper}({}, {})",
                emit_expr_with_result(left, bind_result),
                emit_expr_with_result(right, bind_result)
            )
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                unreachable!()
            };
            let name = match path.segments[0].as_str() {
                "arg" => "_cgx2_arg".to_owned(),
                "arg_count" => "_cgx2_arg_count".to_owned(),
                other => python_name(other),
            };
            let args = arguments
                .iter()
                .map(|arg| emit_expr_with_result(&arg.value, bind_result))
                .collect::<Vec<_>>()
                .join(", ");
            let comparison = match path.segments[0].as_str() {
                "$cgx2_cmp_eq" => Some("=="),
                "$cgx2_cmp_ne" => Some("!="),
                "$cgx2_cmp_lt" => Some("<"),
                "$cgx2_cmp_le" => Some("<="),
                "$cgx2_cmp_gt" => Some(">"),
                "$cgx2_cmp_ge" => Some(">="),
                _ => None,
            };
            if let Some(op) = comparison {
                format!(
                    "({} {op} {})",
                    emit_expr_with_result(&arguments[0].value, bind_result),
                    emit_expr_with_result(&arguments[1].value, bind_result)
                )
            } else {
                format!("{name}({args})")
            }
        }
    };
    format!(
        "(_cgx2_step({}, {}), {value})[1]",
        expr.span.start, expr.span.end
    )
}

fn decode_string(raw: &str) -> Option<String> {
    let content = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = content.chars();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            });
        } else {
            result.push(ch);
        }
    }
    Some(result)
}

fn quote_python(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch.is_control() && (ch as u32) <= 0xffff => {
                result.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch if ch.is_control() => result.push_str(&format!("\\U{:08x}", ch as u32)),
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}
