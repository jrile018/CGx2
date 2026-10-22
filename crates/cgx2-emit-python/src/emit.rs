use cgx2_sema::{check_module, TypedModule};
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Stmt, StmtKind, TypeSyntax};

use crate::error::EmitDiagnostic;
use crate::names::python_name;

const PRELUDE: &str = r#"# Generated from CGx2's checked scalar profile.
import builtins as _cgx2_builtins
import sys as _cgx2_sys

_cgx2_min = -9223372036854775808
_cgx2_max = 9223372036854775807

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

pub(crate) fn emit(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
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
    if !module.functions.is_empty() {
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
    for function in &module.functions {
        validate_metadata(function, typed, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut output = String::from(PRELUDE);
    for function in &module.functions {
        let params = function
            .parameters
            .iter()
            .map(|parameter| python_name(&parameter.name))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("def {}({params}):\n", python_name(&function.name)));
        if function.requires.is_empty() && function.ensures.is_empty() {
            emit_block(&function.body, 1, true, &mut output);
        } else {
            for condition in &function.requires {
                emit_contract_check(condition, "CGR015", 1, false, &mut output);
            }
            output.push_str("    def _cgx2_contract_body():\n");
            emit_block(&function.body, 2, true, &mut output);
            output.push_str("    _cgx2_contract_result = _cgx2_contract_body()\n");
            for condition in &function.ensures {
                emit_contract_check(condition, "CGR016", 1, true, &mut output);
            }
            output.push_str("    return _cgx2_contract_result\n");
        }
        output.push('\n');
    }
    if module.functions.is_empty() {
        output.push_str("def _cgx2_script():\n");
        emit_block(&module.statements, 1, true, &mut output);
        output.push('\n');
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
            if !matches!(effect.as_str(), "Mutate" | "ReadArgs") {
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
            StmtKind::For { .. } => errors.push(EmitDiagnostic::new(
                "CGE001",
                "for loops are unsupported by Python emission",
                stmt.span,
            )),
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

fn emit_block(stmts: &[Stmt], indent: usize, return_tail: bool, output: &mut String) {
    for (index, stmt) in stmts.iter().enumerate() {
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
            StmtKind::For { .. } => unreachable!("validated before emission"),
            StmtKind::NoGc { .. } => unreachable!("validated before emission"),
        }
    }
    if stmts.is_empty()
        || !matches!(
            stmts.last().map(|s| &s.kind),
            Some(StmtKind::Return(_) | StmtKind::Expr(_))
        )
    {
        output.push_str(&"    ".repeat(indent));
        output.push_str("return None\n");
    }
}

fn emit_expr(expr: &Expr) -> String {
    emit_expr_with_result(expr, false)
}

fn emit_expr_with_result(expr: &Expr, bind_result: bool) -> String {
    match &expr.kind {
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
                return format!(
                    "({} {op} {})",
                    emit_expr_with_result(&arguments[0].value, bind_result),
                    emit_expr_with_result(&arguments[1].value, bind_result)
                );
            }
            format!("{name}({args})")
        }
    }
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
