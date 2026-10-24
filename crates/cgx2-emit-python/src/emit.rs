use cgx2_sema::{check_module, Type, TypedModule};
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Record, Stmt, StmtKind, TypeSyntax};
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


def _cgx2_math_error(kind, code, message, start, end):
    error = kind(f"{code} at {start}..{end}: {message}")
    error.code = code
    error.span = (start, end)
    return error
def _cgx2_i64(value, start=0, end=0):
    if _cgx2_builtins.type(value) is not _cgx2_builtins.int or not (_cgx2_min <= value <= _cgx2_max):
        raise _cgx2_math_error(_cgx2_builtins.OverflowError, "CGR005", "i64 arithmetic overflow", start, end)
    return value

def _cgx2_add(left, right, start=0, end=0):
    return _cgx2_i64(left + right, start, end)

def _cgx2_sub(left, right, start=0, end=0):
    return _cgx2_i64(left - right, start, end)

def _cgx2_mul(left, right, start=0, end=0):
    return _cgx2_i64(left * right, start, end)

def _cgx2_div(left, right, start=0, end=0):
    if right == 0:
        raise _cgx2_math_error(_cgx2_builtins.ZeroDivisionError, "CGR006", "division by zero", start, end)
    quotient = _cgx2_builtins.abs(left) // _cgx2_builtins.abs(right)
    if (left < 0) != (right < 0):
        quotient = -quotient
    return _cgx2_i64(quotient, start, end)

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
    return _cgx2_builtins.str(value)

class _cgx2_ContractError(_cgx2_builtins.RuntimeError):
    def __init__(self, code, start, end):
        self.code = code
        self.span = (start, end)
        super().__init__(f"{code}: contract condition failed at bytes [{start}, {end})")

def _cgx2_contract_check(value, code, start, end):
    if value is not True:
        raise _cgx2_ContractError(code, start, end)

"#;

const PACKAGE_HELPERS: &str = "# Generated from a checked CGx2 module.\nfrom _cgx2_support import _cgx2_add, _cgx2_sub, _cgx2_mul, _cgx2_div\nfrom _cgx2_support import _cgx2_arg, _cgx2_arg_count, _cgx2_show, _cgx2_contract_check\nfrom _cgx2_support import _cgx2_step, _cgx2_enter, _cgx2_leave\nfrom _cgx2_support import _cgx2_builtins, _cgx2_i64\n\n";

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
    if let Some(span) = cgx2_syntax::s2a_feature_span(module) {
        return Err(vec![EmitDiagnostic::new("CGE006","Python target does not yet support S2a variants, generic applications, collections/indexing or annotated loops (S2b)",span)]);
    }
    let original_module = module;
    let mut normalized = if package_mode {
        module.clone()
    } else {
        cgx2_syntax::canonical_module(module, &BTreeSet::new())
    };
    if !package_mode {
        if let Some(name) = &normalized.name {
            for record in &mut normalized.records {
                record.name = format!("{}.{}", name.segments.join("."), record.name);
            }
        }
    }
    let module = &normalized;
    let mut errors = Vec::new();
    let mut record_names = module
        .records
        .iter()
        .map(|record| record.name.clone())
        .collect::<BTreeSet<_>>();
    if package_mode {
        for signature in typed.functions.values() {
            for ty in signature
                .parameters
                .iter()
                .chain(std::iter::once(&signature.return_type))
            {
                if let Type::Record(name) = ty {
                    record_names.insert(name.clone());
                }
            }
        }
        for (_, ty) in typed.expressions.iter() {
            if let Type::Record(name) = ty {
                record_names.insert(name.clone());
            }
        }
    }
    for record in &module.records {
        if !(1..=8).contains(&record.fields.len())
            || record
                .fields
                .iter()
                .any(|field| !matches!(field.ty.to_string().as_str(), "i64" | "Bool"))
        {
            errors.push(EmitDiagnostic::new(
                "CGE001",
                "only immutable records with one to eight i64/Bool fields are supported",
                record.span,
            ));
        }
    }
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
            validate_type(&parameter.ty, &record_names, &mut errors);
        }
        if let Some(ty) = &function.return_type {
            validate_type(ty, &record_names, &mut errors);
        }
        for contract in function.requires.iter().chain(&function.ensures) {
            validate_expr(contract, &record_names, &mut errors);
        }
        validate_block(&function.body, false, &record_names, &mut errors);
    }
    validate_block(&module.statements, true, &record_names, &mut errors);
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
        match check_module(original_module) {
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
    if !package_mode {
        validate_record_bindings(module, typed, &mut errors);
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
    for record in &module.records {
        emit_record(record, &mut output);
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
        for parameter in &function.parameters {
            if record_names.contains(&parameter.ty.path.segments.join(".")) {
                output.push_str(&format!("        if _cgx2_builtins.type({}) is not {}: raise _cgx2_builtins.TypeError('CGR004: wrong nominal record input')\n", python_name(&parameter.name), python_name(&parameter.ty.path.segments.join("."))));
            }
        }
        let record_result = function
            .return_type
            .as_ref()
            .filter(|ty| record_names.contains(&ty.path.segments.join(".")));

        if function.requires.is_empty() && function.ensures.is_empty() && record_result.is_none() {
            emit_block(&function.body, 2, true, &mut output);
        } else {
            for condition in &function.requires {
                emit_contract_check(condition, "CGR015", 2, false, &mut output);
            }
            output.push_str("        def _cgx2_contract_body():\n");
            emit_block(&function.body, 3, true, &mut output);
            output.push_str("        _cgx2_contract_result = _cgx2_contract_body()\n");
            if let Some(ty) = record_result {
                output.push_str(&format!("        if _cgx2_builtins.type(_cgx2_contract_result) is not {}: raise _cgx2_builtins.TypeError('CGR004: wrong nominal record result')\n", python_name(&ty.path.segments.join("."))));
            }

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

fn record_type(ty: &TypeSyntax, records: &[Record]) -> Option<String> {
    if ty.arguments.is_empty() {
        let name = ty.path.segments.join(".");
        if records.iter().any(|record| record.name == name) {
            return Some(name);
        }
    }
    None
}

fn expression_record(expr: &Expr, typed: &TypedModule) -> Option<String> {
    match typed.expressions.get(expr.span) {
        Some(Type::Record(name)) => Some(name.clone()),
        _ => None,
    }
}

type RecordScopes = Vec<BTreeMap<String, Option<String>>>;

fn lookup_record(scopes: &RecordScopes, name: &str) -> Option<Option<String>> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| scope.get(name).cloned())
}

fn validate_record_expr(
    expr: &Expr,
    scopes: &RecordScopes,
    records: &[Record],
    errors: &mut Vec<EmitDiagnostic>,
) {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() == 2 => {
            let [base, field] = path.segments.as_slice() else {
                unreachable!()
            };
            let valid = lookup_record(scopes, base)
                .flatten()
                .and_then(|record_name| records.iter().find(|record| record.name == record_name))
                .is_some_and(|record| record.fields.iter().any(|item| item.name == *field));
            if !valid {
                errors.push(EmitDiagnostic::new(
                    "CGE001",
                    "qualified name is not a local record field",
                    path.span,
                ));
            }
        }
        ExprKind::Binary { left, right, .. } => {
            validate_record_expr(left, scopes, records, errors);
            validate_record_expr(right, scopes, records, errors);
        }
        ExprKind::Call { arguments, .. } => {
            for arg in arguments {
                validate_record_expr(&arg.value, scopes, records, errors);
            }
        }
        _ => {}
    }
}

fn validate_record_block(
    stmts: &[Stmt],
    scopes: &mut RecordScopes,
    records: &[Record],
    typed: &TypedModule,
    errors: &mut Vec<EmitDiagnostic>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
                unreachable!("S2a profile rejected before Python lowering")
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                validate_record_expr(condition, scopes, records, errors);
                for body in [then_body, else_body] {
                    scopes.push(BTreeMap::new());
                    validate_record_block(body, scopes, records, typed, errors);
                    scopes.pop();
                }
            }
            StmtKind::Let {
                mutable,
                name,
                annotation,
                value,
            } => {
                validate_record_expr(value, scopes, records, errors);
                let record = annotation
                    .as_ref()
                    .and_then(|ty| record_type(ty, records))
                    .or_else(|| expression_record(value, typed));
                if *mutable && record.is_some() {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "mutable record bindings unsupported by Python emission",
                        stmt.span,
                    ));
                }
                scopes
                    .last_mut()
                    .expect("record scope")
                    .insert(name.clone(), record);
            }
            StmtKind::Assign { value, .. }
            | StmtKind::Return(Some(value))
            | StmtKind::Expr(value) => {
                validate_record_expr(value, scopes, records, errors);
            }
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                validate_record_expr(iter, scopes, records, errors);
                scopes.push(BTreeMap::from([(binding.clone(), None)]));
                validate_record_block(body, scopes, records, typed, errors);
                scopes.pop();
            }
            StmtKind::NoGc { budget, body } => {
                validate_record_expr(budget, scopes, records, errors);
                scopes.push(BTreeMap::new());
                validate_record_block(body, scopes, records, typed, errors);
                scopes.pop();
            }
            StmtKind::Return(None) => {}
        }
    }
}

fn validate_record_bindings(
    module: &Module,
    typed: &TypedModule,
    errors: &mut Vec<EmitDiagnostic>,
) {
    if module.records.is_empty() {
        return;
    }
    for function in &module.functions {
        let mut scopes = vec![function
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.clone(),
                    record_type(&parameter.ty, &module.records),
                )
            })
            .collect()];
        for condition in &function.requires {
            validate_record_expr(condition, &scopes, &module.records, errors);
        }
        validate_record_block(&function.body, &mut scopes, &module.records, typed, errors);
        let result_record = typed.functions.get(&function.name).and_then(|signature| {
            match &signature.return_type {
                Type::Record(name) => Some(name.clone()),
                _ => None,
            }
        });
        scopes[0].insert("result".to_owned(), result_record);
        for condition in &function.ensures {
            validate_record_expr(condition, &scopes, &module.records, errors);
        }
    }
    validate_record_block(
        &module.statements,
        &mut vec![BTreeMap::new()],
        &module.records,
        typed,
        errors,
    );
}

fn emit_record(record: &Record, output: &mut String) {
    let class = python_name(&record.name);
    let fields = record
        .fields
        .iter()
        .map(|field| python_name(&field.name))
        .collect::<Vec<_>>();
    output.push_str(&format!("class {class}:\n"));
    output.push_str(&format!(
        "    __slots__ = ({},)\n",
        fields
            .iter()
            .map(|field| format!("{field:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    output.push_str(&format!(
        "    def __init__(_cgx2_self, *, {}):\n",
        fields.join(", ")
    ));
    for field in &fields {
        output.push_str(&format!(
            "        _cgx2_builtins.object.__setattr__(_cgx2_self, {field:?}, {field})\n"
        ));
    }
    output.push_str("    def __setattr__(self, name, value):\n        raise _cgx2_builtins.AttributeError('immutable CGx2 record')\n");
    output.push_str("    def __delattr__(self, name):\n        raise _cgx2_builtins.AttributeError('immutable CGx2 record')\n");
    output.push_str("    def __str__(self):\n");
    let parts = record
        .fields
        .iter()
        .zip(&fields)
        .map(|(field, py_field)| format!("{}: {{_cgx2_show(self.{py_field})}}", field.name))
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!("        return f\"{}({parts})\"\n", record.name));
    output.push_str("    __repr__ = __str__\n\n");
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

fn validate_type(ty: &TypeSyntax, records: &BTreeSet<String>, errors: &mut Vec<EmitDiagnostic>) {
    let name = ty.path.segments.join(".");
    if !ty.arguments.is_empty()
        || !(matches!(name.as_str(), "i64" | "Bool" | "String" | "Unit") || records.contains(&name))
    {
        errors.push(EmitDiagnostic::new(
            "CGE001",
            format!("type {ty} is unsupported by Python emission"),
            ty.span,
        ));
    }
}

fn validate_block(
    stmts: &[Stmt],
    script: bool,
    records: &BTreeSet<String>,
    errors: &mut Vec<EmitDiagnostic>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
                unreachable!("S2a profile rejected before Python lowering")
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                validate_expr(condition, records, errors);
                validate_block(then_body, script, records, errors);
                validate_block(else_body, script, records, errors);
            }
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    validate_type(ty, records, errors);
                }
                validate_expr(value, records, errors);
            }
            StmtKind::Assign { target, value, .. } => {
                if target.segments.len() != 1 {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "qualified assignment is unsupported by Python emission",
                        target.span,
                    ));
                }
                validate_expr(value, records, errors);
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
                    validate_expr(expr, records, errors);
                }
            }
            StmtKind::Expr(expr) => validate_expr(expr, records, errors),
            StmtKind::For { iter, body, .. } => {
                let valid = matches!(&iter.kind, ExprKind::Call { callee, arguments }
                    if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                    && arguments.len() == 2 && arguments.iter().all(|arg| arg.name.is_none()));
                if valid {
                    validate_expr(iter, records, errors);
                    validate_block(body, script, records, errors);
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

fn validate_expr(expr: &Expr, records: &BTreeSet<String>, errors: &mut Vec<EmitDiagnostic>) {
    match &expr.kind {
        ExprKind::Name(path)
            if path.segments.len() != 1 && !(path.segments.len() == 2 && !records.is_empty()) =>
        {
            errors.push(EmitDiagnostic::new(
                "CGE001",
                "qualified names are unsupported by Python emission",
                path.span,
            ))
        }
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
            validate_expr(left, records, errors);
            validate_expr(right, records, errors);
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
            let constructor = matches!(&callee.kind, ExprKind::Name(path)
                if path.segments.len() == 1 && records.contains(&path.segments[0]));
            for arg in arguments {
                if arg.name.is_some() != constructor {
                    errors.push(EmitDiagnostic::new(
                        "CGE001",
                        "record constructors require named fields; other calls require positional arguments",
                        arg.span,
                    ));
                }
                validate_expr(&arg.value, records, errors);
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
            StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
                unreachable!("S2a profile rejected before Python lowering")
            }
            StmtKind::If {
                then_body,
                else_body,
                ..
            } => {
                collect_bindings(then_body, used);
                collect_bindings(else_body, used);
            }
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
        ExprKind::Name(path) if matches!(path.segments.len(), 1 | 2) => {
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
            StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
                unreachable!("S2a profile rejected before Python lowering")
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                scope_expr(condition, scopes);
                for body in [then_body, else_body] {
                    scopes.push(BTreeMap::new());
                    scope_block(body, scopes, used, assigned, reserved, next);
                    scopes.pop();
                }
            }
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
    for record in &module.records {
        used.insert(record.name.clone());
        reserved.insert(record.name.clone());
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
            StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
                unreachable!("S2a profile rejected before Python lowering")
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                output.push_str(&format!("if {}:\n", emit_expr(condition)));
                emit_block(then_body, indent + 1, false, output);
                output.push_str(&format!("{}else:\n", "    ".repeat(indent)));
                emit_block(else_body, indent + 1, false, output);
            }
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
        ExprKind::Constructor { .. } | ExprKind::Collection(_) | ExprKind::Index { .. } => {
            unreachable!("S2a profile rejected before Python lowering")
        }
        ExprKind::Name(path) if bind_result && path.segments[0] == "result" => {
            if path.segments.len() == 2 {
                format!("_cgx2_contract_result.{}", python_name(&path.segments[1]))
            } else {
                "_cgx2_contract_result".to_owned()
            }
        }
        ExprKind::Name(path) if path.segments.len() == 2 => format!(
            "{}.{}",
            python_name(&path.segments[0]),
            python_name(&path.segments[1])
        ),
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
                "{helper}({}, {}, {}, {})",
                emit_expr_with_result(left, bind_result),
                emit_expr_with_result(right, bind_result),
                expr.span.start,
                expr.span.end
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
                .map(|arg| {
                    let value = emit_expr_with_result(&arg.value, bind_result);
                    match &arg.name {
                        Some(field) => format!("{}={value}", python_name(field)),
                        None => value,
                    }
                })
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

pub(crate) fn decode_string(raw: &str) -> Option<String> {
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

pub(crate) fn quote_python(value: &str) -> String {
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

#[cfg(test)]
mod record_tests {
    use super::emit_package_module;
    use cgx2_sema::check_module;
    use cgx2_syntax::parse_module;

    #[test]
    fn package_record_class_is_emitted_with_checked_metadata() {
        let module = parse_module(
            "record Point:\n    x: i64\nfn main() -> i64:\n    let p = Point(x: 1)\n    p.x\n",
        );
        let module = module.unwrap();
        let typed = check_module(&module).unwrap();
        let python = emit_package_module(&module, &typed, "").unwrap();
        assert!(python.contains("class Point:"));
    }
}
