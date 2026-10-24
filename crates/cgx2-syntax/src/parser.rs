use std::{collections::BTreeSet, mem::discriminant};

use crate::{
    lex, AssignOp, BinaryOp, CallArg, Declaration, DeclaredName, Expr, ExprKind, ForeignBinding,
    Function, Module, Parameter, Path, Record, RecordField, Span, Stmt, StmtKind, SyntaxDiagnostic,
    Token, TokenKind, TypeSyntax,
};

/// Parse a candidate CGx2 module or standalone script.
pub fn parse_module(source: &str) -> Result<Module, Vec<SyntaxDiagnostic>> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        type_depth: 0,
        loop_depth: 0,
        errors: Vec::new(),
    };
    let name = if parser.at(&TokenKind::Module) {
        parser.advance();
        let path = parser.parse_path();
        parser.end_statement();
        path
    } else {
        None
    };
    let mut imports = Vec::new();
    let mut seen_imports = BTreeSet::new();
    while parser.at(&TokenKind::Import) {
        parser.advance();
        if let Some(path) = parser.parse_path() {
            let dotted = path.segments.join(".");
            if !seen_imports.insert(dotted.clone()) {
                parser.errors.push(SyntaxDiagnostic {
                    code: "CGS108",
                    message: format!("duplicate import `{dotted}`"),
                    span: path.span,
                });
            }
            imports.push(path);
        }
        parser.end_statement();
    }
    let mut records = Vec::new();
    let mut variants = Vec::new();
    let mut functions = Vec::new();
    let mut statements = Vec::new();
    while !parser.at(&TokenKind::Eof) {
        let before = parser.pos;
        if (parser.at_name("variant")
            && parser
                .tokens
                .get(parser.pos + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::Name(_))))
            || (parser.at(&TokenKind::Pub)
                && parser
                    .tokens
                    .get(parser.pos + 1)
                    .is_some_and(|t| matches!(&t.kind, TokenKind::Name(n) if n == "variant")))
        {
            if let Some(variant) = parser.parse_variant() {
                variants.push(variant);
            }
        } else if parser.at_name("record")
            || (parser.at(&TokenKind::Pub)
                && parser.tokens.get(parser.pos + 1).is_some_and(
                    |token| matches!(&token.kind, TokenKind::Name(name) if name == "record"),
                ))
        {
            if let Some(record) = parser.parse_record() {
                records.push(record);
            }
        } else if parser.at_name("unsafe")
            || parser.at_name("foreign")
            || parser.at(&TokenKind::Pub)
            || parser.at(&TokenKind::Fn)
        {
            if let Some(function) = parser.parse_function() {
                if function.foreign.is_some()
                    && functions
                        .iter()
                        .any(|prior: &Function| prior.name == function.name)
                {
                    parser.errors.push(SyntaxDiagnostic {
                        code: "CGS111",
                        message: format!("duplicate foreign declaration `{}`", function.name),
                        span: function.span,
                    });
                }
                functions.push(function);
            }
        } else if parser.at(&TokenKind::Import) {
            parser.error(
                "CGS109",
                "imports must precede functions and script statements",
            );
            parser.recover();
        } else if parser.at(&TokenKind::Indent) || parser.at(&TokenKind::Dedent) {
            parser.error("CGS102", "unexpected indentation boundary");
            parser.advance();
        } else if parser.at(&TokenKind::Newline) {
            parser.advance();
        } else if let Some(stmt) = parser.parse_stmt() {
            statements.push(stmt);
        }
        if parser.pos == before {
            parser.recover();
        }
    }
    let module = Module {
        name,
        imports,
        records,
        variants,
        functions,
        statements,
        span: Span {
            start: 0,
            end: source.len(),
        },
    };
    if !module.functions.is_empty() && !module.statements.is_empty() {
        let span = module
            .statements
            .first()
            .map(|stmt| stmt.span)
            .unwrap_or(module.span);
        parser.errors.push(SyntaxDiagnostic {
            code: "CGS105",
            message: "function items and top-level script statements cannot be mixed in this candidate grammar".into(),
            span,
        });
    }
    if parser.errors.is_empty() {
        Ok(module)
    } else {
        Err(parser.errors)
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    type_depth: usize,
    loop_depth: usize,
    errors: Vec<SyntaxDiagnostic>,
}

impl Parser {
    fn parse_type_parameters(&mut self) -> Option<Vec<crate::TypeParameter>> {
        let mut parameters = Vec::new();
        if self.eat(&TokenKind::LAngle).is_none() {
            return Some(parameters);
        }
        loop {
            let (name, span) = self.name()?;
            let mut constraints = Vec::new();
            if self.eat(&TokenKind::Colon).is_some() {
                loop {
                    constraints.push(self.parse_path()?);
                    if self.eat(&TokenKind::Plus).is_none() {
                        break;
                    }
                }
            }
            let end = constraints.last().map_or(span.end, |p| p.span.end);
            if parameters
                .iter()
                .any(|p: &crate::TypeParameter| p.name == name)
            {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS113",
                    message: format!("duplicate type parameter `{name}`"),
                    span,
                });
            }
            parameters.push(crate::TypeParameter {
                name,
                constraints,
                span: Span {
                    start: span.start,
                    end,
                },
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(
            &TokenKind::RAngle,
            "CGS113",
            "expected > after type parameters",
        )?;
        Some(parameters)
    }

    fn parse_variant(&mut self) -> Option<crate::Variant> {
        let start = self.peek().span.start;
        let public = self.eat(&TokenKind::Pub).is_some();
        self.advance();
        let (name, _) = self.name()?;
        let type_parameters = self.parse_type_parameters()?;
        self.expect(&TokenKind::Colon, "CGS114", "expected : after variant")?;
        self.expect(
            &TokenKind::Newline,
            "CGS114",
            "expected newline after variant",
        )?;
        self.expect(&TokenKind::Indent, "CGS114", "expected indented cases")?;
        let mut cases = Vec::new();
        let mut seen = BTreeSet::new();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let (name, span) = self.name()?;
            if !seen.insert(name.clone()) {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS114",
                    message: format!("duplicate variant case `{name}`"),
                    span,
                });
            }
            let mut payloads = Vec::new();
            if self.eat(&TokenKind::LParen).is_some() {
                if !self.at(&TokenKind::RParen) {
                    loop {
                        payloads.push(self.parse_type()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect(
                    &TokenKind::RParen,
                    "CGS114",
                    "expected ) after payload types",
                )?;
            }
            let end = self.end_statement()?;
            cases.push(crate::VariantCase {
                name,
                payloads,
                span: Span {
                    start: span.start,
                    end,
                },
            });
        }
        let end = self
            .expect(&TokenKind::Dedent, "CGS114", "expected end of variant")?
            .span
            .end;
        if cases.is_empty() {
            self.error("CGS114", "variant requires at least one case");
        }
        Some(crate::Variant {
            public,
            name,
            type_parameters,
            cases,
            span: Span { start, end },
        })
    }

    fn parse_match(&mut self) -> Option<Stmt> {
        let start = self.advance().span.start;
        let scrutinee = self.parse_expr(0)?;
        self.expect(
            &TokenKind::Colon,
            "CGS116",
            "expected : after match scrutinee",
        )?;
        self.expect(
            &TokenKind::Newline,
            "CGS116",
            "expected newline after match",
        )?;
        self.expect(&TokenKind::Indent, "CGS116", "expected indented match arms")?;
        let mut arms = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let arm_start = self.peek().span.start;
            let pattern = if self.at_name("_") {
                crate::Pattern::Wildcard(self.advance().span)
            } else {
                let mut ty = self.parse_type()?;
                let case = if self.eat(&TokenKind::Dot).is_some() {
                    self.name()?.0
                } else {
                    if ty.path.segments.len() < 2 {
                        self.error("CGS116", "pattern requires a qualified variant case");
                        return None;
                    }
                    let case = ty.path.segments.pop()?;
                    ty.path.span.end -= case.len() + 1;
                    ty.span.end = ty.path.span.end;
                    case
                };
                let mut bindings = Vec::new();
                if self.eat(&TokenKind::LParen).is_some() {
                    if !self.at(&TokenKind::RParen) {
                        loop {
                            let (name, span) = self.name()?;
                            bindings.push(DeclaredName { name, span });
                            if self.eat(&TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenKind::RParen, "CGS116", "nested patterns and guards are unsupported; expected ) after binding names")?;
                }
                crate::Pattern::Case {
                    ty,
                    case,
                    bindings,
                    span: Span {
                        start: arm_start,
                        end: self.peek().span.start,
                    },
                }
            };
            if self.at_name("if") || self.at(&TokenKind::If) {
                self.error("CGS116", "match guards are unsupported");
                return None;
            }
            let (body, end) = self.parse_suite()?;
            arms.push(crate::MatchArm {
                pattern,
                body,
                span: Span {
                    start: arm_start,
                    end,
                },
            });
        }
        let end = self
            .expect(&TokenKind::Dedent, "CGS116", "expected end of match")?
            .span
            .end;
        Some(Stmt {
            kind: StmtKind::Match { scrutinee, arms },
            span: Span { start, end },
        })
    }

    fn parse_record(&mut self) -> Option<Record> {
        let start = self.peek().span.start;
        let public = self.eat(&TokenKind::Pub).is_some();
        self.advance();
        let (name, _) = self.name()?;
        let type_parameters = self.parse_type_parameters()?;
        self.expect(&TokenKind::Colon, "CGS101", "expected : after record name")?;
        self.expect(
            &TokenKind::Newline,
            "CGS103",
            "expected newline after record header",
        )?;
        let indent = self.expect(&TokenKind::Indent, "CGS104", "expected record fields")?;
        if indent.span.end - indent.span.start != 4 {
            self.errors.push(SyntaxDiagnostic {
                code: "CGS104",
                message: "record fields require four-space indentation".into(),
                span: indent.span,
            });
        }
        let mut fields = Vec::new();
        let mut seen = BTreeSet::new();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let (field_name, name_span) = self.name()?;
            if !seen.insert(field_name.clone()) {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS112",
                    message: format!("duplicate record field `{field_name}`"),
                    span: name_span,
                });
            }
            self.expect(&TokenKind::Colon, "CGS103", "expected : after field name")?;
            let ty = self.parse_type()?;
            let end = self.end_statement()?;
            fields.push(RecordField {
                name: field_name,
                ty,
                span: Span {
                    start: name_span.start,
                    end,
                },
            });
        }
        self.expect(&TokenKind::Dedent, "CGS102", "expected end of record")?;
        if !(1..=8).contains(&fields.len()) {
            self.errors.push(SyntaxDiagnostic {
                code: "CGS113",
                message: "record requires one to eight fields".into(),
                span: Span {
                    start,
                    end: self.peek().span.start,
                },
            });
        }
        let end = fields.last().map_or(start, |field| field.span.end);
        Some(Record {
            public,
            name,
            fields,
            type_parameters,
            span: Span { start, end },
        })
    }
    fn at_name(&self, name: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Name(value) if value == name)
    }
    fn at_contract_line(&self) -> bool {
        (self.at_name("requires") || self.at_name("ensures"))
            && self.tokens.get(self.pos + 1).is_some_and(|next| {
                !matches!(next.kind, TokenKind::Newline) && next.span.start > self.peek().span.end
            })
    }
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }
    fn at(&self, kind: &TokenKind) -> bool {
        discriminant(&self.peek().kind) == discriminant(kind)
    }
    fn advance(&mut self) -> Token {
        let result = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        result
    }
    fn eat(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.at(kind) {
            Some(self.advance())
        } else {
            None
        }
    }
    fn error(&mut self, code: &'static str, message: &str) {
        self.errors.push(SyntaxDiagnostic {
            code,
            message: message.into(),
            span: self.peek().span,
        });
    }
    fn expect(&mut self, kind: &TokenKind, code: &'static str, message: &str) -> Option<Token> {
        if self.at(kind) {
            Some(self.advance())
        } else {
            self.error(code, message);
            None
        }
    }
    fn name(&mut self) -> Option<(String, Span)> {
        if let TokenKind::Name(value) = &self.peek().kind {
            let value = value.clone();
            let span = self.advance().span;
            Some((value, span))
        } else {
            self.error("CGS103", "expected name");
            None
        }
    }
    fn parse_path(&mut self) -> Option<Path> {
        let (first, start) = self.name()?;
        let mut segments = vec![first];
        let mut end = start.end;
        while self.eat(&TokenKind::Dot).is_some() {
            let (segment, span) = self.name()?;
            segments.push(segment);
            end = span.end;
        }
        Some(Path {
            segments,
            span: Span {
                start: start.start,
                end,
            },
        })
    }
    fn parse_type(&mut self) -> Option<TypeSyntax> {
        if self.type_depth >= 64 {
            self.error("CGS117", "type nesting exceeds 64");
            return None;
        }
        self.type_depth += 1;
        let result = self.parse_type_inner();
        self.type_depth -= 1;
        result
    }
    fn parse_type_inner(&mut self) -> Option<TypeSyntax> {
        let path = self.parse_path()?;
        let mut end = path.span.end;
        let mut arguments = Vec::new();
        if self.eat(&TokenKind::LAngle).is_some() {
            loop {
                arguments.push(self.parse_type()?);
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            end = self
                .expect(
                    &TokenKind::RAngle,
                    "CGS103",
                    "expected > after type arguments",
                )?
                .span
                .end;
        }
        Some(TypeSyntax {
            span: Span {
                start: path.span.start,
                end,
            },
            path,
            arguments,
        })
    }
    fn parse_function(&mut self) -> Option<Function> {
        let start = self.peek().span.start;
        let unsafe_call = if self.at_name("unsafe") {
            self.advance();
            true
        } else {
            false
        };
        let route = if self.at_name("foreign") {
            self.advance();
            let (route, span) = self.name()?;
            if route != "python" && route != "native" {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS111",
                    message: format!("unsupported foreign route `{route}`"),
                    span,
                });
            }
            Some(route)
        } else {
            if unsafe_call {
                self.error("CGS111", "unsafe requires a foreign native declaration");
            }
            None
        };
        let public = self.eat(&TokenKind::Pub).is_some();
        self.expect(&TokenKind::Fn, "CGS103", "expected fn")?;
        let (name, _) = self.name()?;
        let type_parameters = self.parse_type_parameters()?;
        self.expect(
            &TokenKind::LParen,
            "CGS103",
            "expected ( after function name",
        )?;
        let mut parameters = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let (param_name, param_span) = self.name()?;
                self.expect(
                    &TokenKind::Colon,
                    "CGS103",
                    "expected : after parameter name",
                )?;
                let ty = self.parse_type()?;
                parameters.push(Parameter {
                    name: param_name,
                    span: Span {
                        start: param_span.start,
                        end: ty.span.end,
                    },
                    ty,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "CGS103", "expected ) after parameters")?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let mut effects = None;
        let mut capabilities = None;
        while self.at_name("effects") || self.at_name("caps") {
            let is_effects = self.at_name("effects");
            let declaration = self.parse_declaration(is_effects)?;
            let slot = if is_effects {
                &mut effects
            } else {
                &mut capabilities
            };
            if slot.is_some() {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS110",
                    message: "duplicate declaration".into(),
                    span: declaration.span,
                });
            } else {
                *slot = Some(declaration);
            }
        }
        if let Some(route) = route {
            self.expect(
                &TokenKind::Equal,
                "CGS111",
                "expected = before foreign target",
            )?;
            let target = self.parse_path()?;
            if !self.at_name("runtime") {
                self.error("CGS111", "expected runtime identity");
                return None;
            }
            self.advance();
            let runtime = if let TokenKind::StringLiteral(raw) = &self.peek().kind {
                let raw = raw.clone();
                self.advance();
                raw.trim_matches('"').to_owned()
            } else {
                self.error("CGS111", "expected quoted runtime identity");
                return None;
            };
            if !self.at_name("ownership") {
                self.error("CGS111", "expected ownership declaration");
                return None;
            }
            self.advance();
            let (ownership, ownership_span) = self.name()?;
            if ownership != "copied" {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS111",
                    message: "foreign scalar ownership must be copied".into(),
                    span: ownership_span,
                });
            }
            let end = self.end_statement()?;
            return Some(Function {
                type_parameters,
                public,
                name,
                parameters,
                return_type,
                effects,
                capabilities,
                requires: vec![],
                ensures: vec![],
                body: vec![],
                foreign: Some(ForeignBinding {
                    unsafe_call,
                    route,
                    target,
                    runtime,
                    ownership,
                    span: Span { start, end },
                }),
                span: Span { start, end },
            });
        }
        self.expect(&TokenKind::Colon, "CGS101", "expected : before block")?;
        self.expect(
            &TokenKind::Newline,
            "CGS103",
            "expected newline after block header",
        )?;
        self.expect(&TokenKind::Indent, "CGS104", "expected indented block")?;
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        while self.at_contract_line() {
            let pre = self.at_name("requires");
            self.advance();
            let expr = self.parse_expr(0)?;
            self.end_statement()?;
            if pre {
                requires.push(expr);
            } else {
                ensures.push(expr);
            }
        }
        let mut body = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let before = self.pos;
            if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            }
            if self.pos == before {
                self.recover();
            }
        }
        let end = self
            .expect(&TokenKind::Dedent, "CGS102", "unexpected end of block")?
            .span
            .end;
        Some(Function {
            type_parameters,
            public,
            name,
            parameters,
            return_type,
            effects,
            capabilities,
            requires,
            ensures,
            body,
            foreign: None,
            span: Span { start, end },
        })
    }
    fn parse_declaration(&mut self, effects: bool) -> Option<Declaration> {
        let start = self.advance().span.start;
        self.expect(&TokenKind::LParen, "CGS103", "expected ( in declaration")?;
        let mut names = Vec::new();
        let mut seen = BTreeSet::new();
        while !self.at(&TokenKind::RParen) && !self.at(&TokenKind::Eof) {
            let (name, span) = self.name()?;
            let valid = if effects {
                matches!(
                    name.as_str(),
                    "ReadManaged"
                        | "ReadArgs"
                        | "ManagedAlloc"
                        | "Collect"
                        | "Mutate"
                        | "Foreign"
                        | "Unknown"
                )
            } else {
                matches!(
                    name.as_str(),
                    "ArgsRead" | "MemoryCollect" | "ForeignCall" | "native"
                )
            };
            if !valid || !seen.insert(name.clone()) {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS110",
                    message: format!("unknown or duplicate declaration name `{name}`"),
                    span,
                });
            }
            names.push(DeclaredName { name, span });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self
            .expect(&TokenKind::RParen, "CGS103", "expected ) in declaration")?
            .span
            .end;
        Some(Declaration {
            names,
            span: Span { start, end },
        })
    }
    fn parse_suite(&mut self) -> Option<(Vec<Stmt>, usize)> {
        self.expect(&TokenKind::Colon, "CGS101", "expected : before block")?;
        self.expect(
            &TokenKind::Newline,
            "CGS103",
            "expected newline after block header",
        )?;
        self.expect(&TokenKind::Indent, "CGS104", "expected indented block")?;
        let mut body = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let before = self.pos;
            if self.at(&TokenKind::Indent) {
                self.error("CGS102", "unexpected indentation boundary");
                self.advance();
            } else if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            }
            if self.pos == before {
                self.recover();
            }
        }
        let end = self
            .expect(&TokenKind::Dedent, "CGS102", "unexpected end of block")?
            .span
            .end;
        Some((body, end))
    }
    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek().span.start;
        let kind = if self.at_name("match")
            && self.tokens[self.pos..]
                .iter()
                .take_while(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
                .last()
                .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
        {
            return self.parse_match();
        } else if self.loop_depth > 0
            && (self.at_name("invariant") || self.at_name("decreases"))
            && self.tokens.get(self.pos + 1).is_some_and(|t| {
                t.span.start > self.peek().span.end
                    && !matches!(t.kind, TokenKind::Newline | TokenKind::Eof)
            })
        {
            let decreases = self.at_name("decreases");
            self.advance();
            let expression = self.parse_expr(0)?;
            StmtKind::LoopContract {
                decreases,
                expression,
            }
        } else if self.eat(&TokenKind::If).is_some() {
            let condition = self.parse_expr(0)?;
            let (then_body, _) = self.parse_suite()?;
            self.expect(
                &TokenKind::Else,
                "CGS107",
                "if statement requires else block",
            )?;
            let (else_body, end) = self.parse_suite()?;
            return Some(Stmt {
                kind: StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                },
                span: Span { start, end },
            });
        } else if self.at(&TokenKind::Else) {
            self.error("CGS107", "misplaced or duplicate else block");
            self.advance();
            return None;
        } else if self.eat(&TokenKind::NoGc).is_some() {
            self.expect(&TokenKind::LParen, "CGS103", "expected ( after no_gc")?;
            let arguments = self.parse_arguments()?;
            let label_span = arguments.first().map_or(self.peek().span, |arg| arg.span);
            if arguments.len() != 1 || arguments[0].name.as_deref() != Some("budget") {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS106",
                    message: "no_gc requires exactly one budget named argument".into(),
                    span: label_span,
                });
            }
            self.expect(&TokenKind::RParen, "CGS103", "expected ) after budget")?;
            let budget = arguments.into_iter().next()?.value;
            if matches!(&budget.kind, ExprKind::Integer(value) if value.starts_with('-')) {
                self.errors.push(SyntaxDiagnostic {
                    code: "CGS106",
                    message: "no_gc budget must be nonnegative".into(),
                    span: budget.span,
                });
            }
            let (body, end) = self.parse_suite()?;
            return Some(Stmt {
                kind: StmtKind::NoGc { budget, body },
                span: Span { start, end },
            });
        } else if self.eat(&TokenKind::Let).is_some() {
            let mutable = self.eat(&TokenKind::Mut).is_some();
            let (name, _) = self.name()?;
            let annotation = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            self.expect(&TokenKind::Equal, "CGS103", "expected = in let binding")?;
            let value = self.parse_expr(0)?;
            StmtKind::Let {
                mutable,
                name,
                annotation,
                value,
            }
        } else if self.eat(&TokenKind::Return).is_some() {
            let value = if self.at(&TokenKind::Newline) {
                None
            } else {
                Some(self.parse_expr(0)?)
            };
            StmtKind::Return(value)
        } else if self.eat(&TokenKind::For).is_some() {
            let (binding, _) = self.name()?;
            self.expect(&TokenKind::In, "CGS103", "expected in after loop binding")?;
            let iter = self.parse_expr(0)?;
            self.loop_depth += 1;
            let suite = self.parse_suite();
            self.loop_depth -= 1;
            let (body, end) = suite?;
            return Some(Stmt {
                kind: StmtKind::For {
                    binding,
                    iter,
                    body,
                },
                span: Span { start, end },
            });
        } else {
            let target = self.parse_expr(0)?;
            let op = if self.eat(&TokenKind::Equal).is_some() {
                Some(AssignOp::Set)
            } else if self.eat(&TokenKind::PlusEqual).is_some() {
                Some(AssignOp::Add)
            } else {
                None
            };
            if let Some(op) = op {
                let ExprKind::Name(path) = target.kind else {
                    self.error("CGS103", "assignment target must be a name");
                    return None;
                };
                let value = self.parse_expr(0)?;
                StmtKind::Assign {
                    target: path,
                    op,
                    value,
                }
            } else {
                StmtKind::Expr(target)
            }
        };
        let end = self.end_statement()?;
        Some(Stmt {
            kind,
            span: Span { start, end },
        })
    }
    fn end_statement(&mut self) -> Option<usize> {
        if let Some(newline) = self.eat(&TokenKind::Newline) {
            Some(newline.span.end)
        } else {
            self.error("CGS103", "expected newline after statement");
            None
        }
    }
    fn parse_expr(&mut self, min_bp: u8) -> Option<Expr> {
        let mut left = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::LBracket).is_some() {
                let index = self.parse_expr(0)?;
                let end = self
                    .expect(&TokenKind::RBracket, "CGS115", "expected ] after index")?
                    .span
                    .end;
                let start = left.span.start;
                left = Expr {
                    kind: ExprKind::Index {
                        collection: Box::new(left),
                        index: Box::new(index),
                    },
                    span: Span { start, end },
                };
                continue;
            }
            if self.at(&TokenKind::LParen) && 7 >= min_bp {
                self.advance();
                let arguments = self.parse_arguments()?;
                let end = self
                    .expect(
                        &TokenKind::RParen,
                        "CGS103",
                        "expected ) after call arguments",
                    )?
                    .span
                    .end;
                let start = left.span.start;
                left = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(left),
                        arguments,
                    },
                    span: Span { start, end },
                };
                continue;
            }
            let comparison = match self.peek().kind {
                TokenKind::EqualEqual => Some("$cgx2_cmp_eq"),
                TokenKind::NotEqual => Some("$cgx2_cmp_ne"),
                TokenKind::LAngle => Some("$cgx2_cmp_lt"),
                TokenKind::LessEqual => Some("$cgx2_cmp_le"),
                TokenKind::RAngle => Some("$cgx2_cmp_gt"),
                TokenKind::GreaterEqual => Some("$cgx2_cmp_ge"),
                _ => None,
            };
            if let Some(name) = comparison {
                if min_bp > 0 {
                    break;
                }
                let operator = self.advance().span;
                let right = self.parse_expr(1)?;
                let span = Span {
                    start: left.span.start,
                    end: right.span.end,
                };
                left = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(Expr {
                            kind: ExprKind::Name(Path {
                                segments: vec![name.into()],
                                span: operator,
                            }),
                            span: operator,
                        }),
                        arguments: vec![
                            CallArg {
                                name: None,
                                span: left.span,
                                value: left,
                            },
                            CallArg {
                                name: None,
                                span: right.span,
                                value: right,
                            },
                        ],
                    },
                    span,
                };
                continue;
            }
            let (op, bp) = match self.peek().kind {
                TokenKind::Plus => (BinaryOp::Add, 1),
                TokenKind::Minus => (BinaryOp::Subtract, 1),
                TokenKind::Star => (BinaryOp::Multiply, 3),
                TokenKind::Slash => (BinaryOp::Divide, 3),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            self.advance();
            let right = self.parse_expr(bp + 1)?;
            let span = Span {
                start: left.span.start,
                end: right.span.end,
            };
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }
        Some(left)
    }
    fn parse_arguments(&mut self) -> Option<Vec<CallArg>> {
        let mut arguments = Vec::new();
        let mut labels = BTreeSet::new();
        let mut named = false;
        if !self.at(&TokenKind::RParen) {
            loop {
                let start = self.peek().span.start;
                let name = if matches!(self.peek().kind, TokenKind::Name(_))
                    && self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|next| matches!(next.kind, TokenKind::Colon))
                {
                    let (name, span) = self.name()?;
                    self.advance();
                    if !labels.insert(name.clone()) {
                        self.errors.push(SyntaxDiagnostic {
                            code: "CGS106",
                            message: format!("duplicate named argument `{name}`"),
                            span,
                        });
                    }
                    named = true;
                    Some(name)
                } else {
                    if named {
                        self.error("CGS107", "positional argument after named argument");
                    }
                    None
                };
                let value = self.parse_expr(0)?;
                arguments.push(CallArg {
                    name,
                    span: Span {
                        start,
                        end: value.span.end,
                    },
                    value,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        Some(arguments)
    }
    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::LBracket => {
                self.advance();
                let mut values = Vec::new();
                if !self.at(&TokenKind::RBracket) {
                    loop {
                        values.push(self.parse_expr(0)?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self
                    .expect(
                        &TokenKind::RBracket,
                        "CGS115",
                        "expected ] after collection",
                    )?
                    .span
                    .end;
                Some(Expr {
                    kind: ExprKind::Collection(values),
                    span: Span {
                        start: token.span.start,
                        end,
                    },
                })
            }
            TokenKind::Name(_) => {
                let saved = self.pos;
                let path = self.parse_path()?;
                if self.at(&TokenKind::LAngle) {
                    let mut depth = 0usize;
                    let mut generic_constructor = false;
                    for (offset, token) in self.tokens[self.pos..].iter().enumerate() {
                        match token.kind {
                            TokenKind::LAngle => depth += 1,
                            TokenKind::RAngle => {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    generic_constructor =
                                        self.tokens.get(self.pos + offset + 1).is_some_and(|t| {
                                            matches!(t.kind, TokenKind::Dot | TokenKind::LParen)
                                        });
                                    break;
                                }
                            }
                            TokenKind::Newline | TokenKind::Eof => break,
                            _ => {}
                        }
                    }
                    if generic_constructor {
                        self.pos = saved;
                        let ty = self.parse_type()?;
                        let case = if self.eat(&TokenKind::Dot).is_some() {
                            self.name()?.0
                        } else {
                            String::new()
                        };
                        self.expect(
                            &TokenKind::LParen,
                            "CGS114",
                            "constructor requires parentheses",
                        )?;
                        let arguments = self.parse_arguments()?;
                        let end = self
                            .expect(&TokenKind::RParen, "CGS114", "expected ) after payloads")?
                            .span
                            .end;
                        return Some(Expr {
                            kind: ExprKind::Constructor {
                                ty,
                                case,
                                arguments,
                            },
                            span: Span {
                                start: path.span.start,
                                end,
                            },
                        });
                    }
                }
                Some(Expr {
                    span: path.span,
                    kind: ExprKind::Name(path),
                })
            }
            TokenKind::Integer(value) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Integer(value),
                    span: token.span,
                })
            }
            TokenKind::Minus => {
                self.advance();
                let integer = self.peek().clone();
                let TokenKind::Integer(value) = integer.kind else {
                    self.error("CGS103", "expected integer literal after unary minus");
                    return None;
                };
                self.advance();
                Some(Expr {
                    kind: ExprKind::Integer(format!("-{value}")),
                    span: Span {
                        start: token.span.start,
                        end: integer.span.end,
                    },
                })
            }
            TokenKind::StringLiteral(value) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::StringLiteral(value),
                    span: token.span,
                })
            }
            TokenKind::True | TokenKind::False => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Boolean(matches!(token.kind, TokenKind::True)),
                    span: token.span,
                })
            }
            TokenKind::LParen => {
                self.advance();
                let mut expr = self.parse_expr(0)?;
                let end = self
                    .expect(&TokenKind::RParen, "CGS103", "expected )")?
                    .span
                    .end;
                expr.span = Span {
                    start: token.span.start,
                    end,
                };
                Some(expr)
            }
            _ => {
                self.error("CGS103", "expected expression");
                None
            }
        }
    }
    fn recover(&mut self) {
        while !self.at(&TokenKind::Newline)
            && !self.at(&TokenKind::Dedent)
            && !self.at(&TokenKind::Eof)
        {
            self.advance();
        }
        if self.at(&TokenKind::Newline) || self.at(&TokenKind::Dedent) {
            self.advance();
        }
    }
}
