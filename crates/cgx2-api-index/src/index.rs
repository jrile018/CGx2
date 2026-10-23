use cgx2_sema::{check_module, check_module_with_imports, ImportedSignatures, TypedModule};
use cgx2_syntax::{format_expression, Module, Span};

use crate::IndexError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiIndex {
    pub schema_version: u32,
    pub module: Option<String>,
    pub functions: Vec<ApiFunction>,
    pub records: Vec<ApiRecord>,
    pub variants: Vec<ApiVariant>,
    pub semantics_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRecord {
    pub name: String,
    pub identity: String,
    pub type_parameters: Vec<ApiTypeParameter>,
    pub fields: Vec<ApiParameter>,
    pub field_types: Vec<ApiType>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiType {
    pub identity: String,
    pub arguments: Vec<ApiType>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiTypeParameter {
    pub name: String,
    pub constraints: Vec<String>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiVariantCase {
    pub name: String,
    pub payloads: Vec<ApiType>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiVariant {
    pub name: String,
    pub identity: String,
    pub type_parameters: Vec<ApiTypeParameter>,
    pub cases: Vec<ApiVariantCase>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiLoopContract {
    pub kind: String,
    pub expression: String,
    pub span: Span,
    pub status: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiFunction {
    pub name: String,
    pub type_parameters: Vec<ApiTypeParameter>,
    pub loop_contracts: Vec<ApiLoopContract>,
    pub parameters: Vec<ApiParameter>,
    pub return_type: String,
    pub parameter_types: Vec<ApiType>,
    pub result_type: Option<ApiType>,
    pub span: Span,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub effects_status: String,
    pub capabilities_status: String,
    pub requires: Vec<ApiContract>,
    pub ensures: Vec<ApiContract>,
    pub foreign: Option<ApiForeign>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiForeign {
    pub route: String,
    pub target: String,
    pub ownership: String,
    pub runtime: String,
    pub unsafe_call: bool,
    pub binding_id: Option<String>,
    /// The only source tensor profile; handles remain inside the direct call.
    pub profile: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiContract {
    pub expression: String,
    pub status: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiParameter {
    pub name: String,
    pub ty: String,
}

/// Build an index only when `typed` equals a fresh semantic check of `module`.
pub fn build_index(module: &Module, typed: &TypedModule) -> Result<ApiIndex, IndexError> {
    let checked = check_module(module).map_err(IndexError::InvalidModule)?;
    build_index_checked(module, typed, checked)
}

/// Recheck a package module against exactly the supplied imported declarations.
pub fn build_index_with_imports(
    module: &Module,
    typed: &TypedModule,
    imports: &ImportedSignatures,
) -> Result<ApiIndex, IndexError> {
    let checked = check_module_with_imports(module, imports).map_err(IndexError::InvalidModule)?;
    build_index_checked(module, typed, checked)
}

fn build_index_checked(
    module: &Module,
    typed: &TypedModule,
    checked: TypedModule,
) -> Result<ApiIndex, IndexError> {
    if &checked != typed {
        return Err(IndexError::MismatchedTypedModule);
    }

    let mut functions = module
        .functions
        .iter()
        .filter(|function| function.public)
        .map(|function| {
            // The equality check above establishes that every source function has
            // a matching signature with the same arity and resolved types.
            let signature = &checked.functions[&function.name];
            ApiFunction {
                name: function.name.clone(),
                type_parameters: parameters(&function.type_parameters),
                loop_contracts: loop_contracts(function),
                parameter_types: function
                    .parameters
                    .iter()
                    .map(|p| api_type(&p.ty, module))
                    .collect(),
                result_type: function.return_type.as_ref().map(|ty| api_type(ty, module)),
                parameters: function
                    .parameters
                    .iter()
                    .zip(&signature.parameters)
                    .map(|(parameter, ty)| ApiParameter {
                        name: parameter.name.clone(),
                        ty: ty.to_string(),
                    })
                    .collect(),
                return_type: signature.return_type.to_string(),
                span: function.span,
                effects: function.effects.as_ref().map_or_else(
                    || {
                        checked.effects[&function.name]
                            .effects
                            .iter()
                            .cloned()
                            .collect()
                    },
                    |declaration| {
                        declaration
                            .names
                            .iter()
                            .map(|name| name.name.clone())
                            .collect()
                    },
                ),
                capabilities: function.capabilities.as_ref().map_or_else(
                    || {
                        checked.effects[&function.name]
                            .capabilities
                            .iter()
                            .cloned()
                            .collect()
                    },
                    |declaration| {
                        declaration
                            .names
                            .iter()
                            .map(|name| name.name.clone())
                            .collect()
                    },
                ),
                effects_status: if function.effects.is_some() {
                    "explicit"
                } else {
                    "inferred"
                }
                .into(),
                capabilities_status: if function.capabilities.is_some() {
                    "explicit"
                } else {
                    "inferred"
                }
                .into(),
                requires: function
                    .requires
                    .iter()
                    .map(|expr| ApiContract {
                        expression: format_expression(expr),
                        status: "runtime-checked".into(),
                        span: expr.span,
                    })
                    .collect(),
                ensures: function
                    .ensures
                    .iter()
                    .map(|expr| ApiContract {
                        expression: format_expression(expr),
                        status: "runtime-checked".into(),
                        span: expr.span,
                    })
                    .collect(),
                foreign: function.foreign.as_ref().map(|binding| ApiForeign {
                    route: binding.route.clone(),
                    target: binding.target.segments.join("."),
                    ownership: binding.ownership.clone(),
                    runtime: binding.runtime.clone(),
                    unsafe_call: binding.unsafe_call,
                    binding_id: (binding.route == "native")
                        .then(|| binding.target.segments[0].clone()),
                    profile: (binding.route == "native"
                        && binding
                            .target
                            .segments
                            .get(1)
                            .is_some_and(|name| name == "cgx2_torch_add"))
                    .then(|| "cpu-1d-f64-add-copied-v1".into()),
                }),
            }
        })
        .collect::<Vec<_>>();
    functions.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.span.start.cmp(&b.span.start))
            .then(a.span.end.cmp(&b.span.end))
    });

    Ok(ApiIndex {
        semantics_status: "checked".into(),
        variants: variants(module),
        schema_version: if cgx2_syntax::s2a_feature_span(module).is_some()
            || !module.records.is_empty()
        {
            6
        } else if !module.records.is_empty() {
            5
        } else if module.functions.iter().any(|function| {
            function
                .foreign
                .as_ref()
                .is_some_and(|binding| binding.route == "native")
        }) {
            4
        } else if module
            .functions
            .iter()
            .any(|function| function.foreign.is_some())
        {
            3
        } else if module.functions.iter().any(|function| {
            function.effects.is_some()
                || function.capabilities.is_some()
                || !function.requires.is_empty()
                || !function.ensures.is_empty()
        }) {
            2
        } else {
            1
        },
        module: module.name.as_ref().map(|path| path.segments.join(".")),
        functions,
        records: records(module),
    })
}

impl ApiIndex {
    /// Look up an exact public function name.
    pub fn find(&self, name: &str) -> Option<&ApiFunction> {
        self.functions.iter().find(|function| function.name == name)
    }
}

fn identity(module: &Module, name: &str) -> String {
    module.name.as_ref().map_or_else(
        || name.to_owned(),
        |p| format!("{}.{name}", p.segments.join(".")),
    )
}
fn parameters(parameters: &[cgx2_syntax::TypeParameter]) -> Vec<ApiTypeParameter> {
    parameters
        .iter()
        .map(|p| ApiTypeParameter {
            name: p.name.clone(),
            constraints: p.constraints.iter().map(|p| p.segments.join(".")).collect(),
            span: p.span,
        })
        .collect()
}
fn api_type(ty: &cgx2_syntax::TypeSyntax, module: &Module) -> ApiType {
    api_type_scoped(ty, module, &[])
}
fn api_type_scoped(
    ty: &cgx2_syntax::TypeSyntax,
    module: &Module,
    parameters: &[cgx2_syntax::TypeParameter],
) -> ApiType {
    let mut canonical = ty.clone();
    if !(ty.path.segments.len() == 1 && parameters.iter().any(|p| p.name == ty.path.segments[0])) {
        cgx2_syntax::qualify_type(&mut canonical, module);
    }
    let name = canonical.path.segments.join(".");
    ApiType {
        identity: if matches!(name.as_str(), "Option" | "Result") {
            format!("cgx2.core.{name}")
        } else {
            name
        },
        arguments: ty
            .arguments
            .iter()
            .map(|t| api_type_scoped(t, module, parameters))
            .collect(),
        span: ty.span,
    }
}
fn records(module: &Module) -> Vec<ApiRecord> {
    let mut records: Vec<_> = module
        .records
        .iter()
        .filter(|r| r.public)
        .map(|r| ApiRecord {
            name: r.name.clone(),
            identity: identity(module, &r.name),
            type_parameters: parameters(&r.type_parameters),
            field_types: r
                .fields
                .iter()
                .map(|f| api_type_scoped(&f.ty, module, &r.type_parameters))
                .collect(),
            fields: r
                .fields
                .iter()
                .map(|f| {
                    let mut ty = f.ty.clone();
                    cgx2_syntax::qualify_type(&mut ty, module);
                    ApiParameter {
                        name: f.name.clone(),
                        ty: ty.to_string(),
                    }
                })
                .collect(),
            span: r.span,
        })
        .collect();
    records.sort_by(|a, b| a.identity.cmp(&b.identity));
    records
}
fn variants(module: &Module) -> Vec<ApiVariant> {
    let mut variants: Vec<_> = module
        .variants
        .iter()
        .filter(|v| v.public)
        .map(|v| ApiVariant {
            name: v.name.clone(),
            identity: identity(module, &v.name),
            type_parameters: parameters(&v.type_parameters),
            cases: v
                .cases
                .iter()
                .map(|c| ApiVariantCase {
                    name: c.name.clone(),
                    payloads: c
                        .payloads
                        .iter()
                        .map(|p| api_type_scoped(p, module, &v.type_parameters))
                        .collect(),
                    span: c.span,
                })
                .collect(),
            span: v.span,
        })
        .collect();
    variants.sort_by(|a, b| a.identity.cmp(&b.identity));
    variants
}

/// Syntax metadata only: no type/effect/proof claim. Allows next-stage tools to
/// inspect declarations whose semantics S2a deliberately rejects.
pub fn build_syntax_index(module: &Module) -> ApiIndex {
    let mut functions: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.public)
        .map(|f| ApiFunction {
            name: f.name.clone(),
            type_parameters: parameters(&f.type_parameters),
            loop_contracts: loop_contracts(f),
            parameter_types: f
                .parameters
                .iter()
                .map(|p| api_type_scoped(&p.ty, module, &f.type_parameters))
                .collect(),
            result_type: f
                .return_type
                .as_ref()
                .map(|ty| api_type_scoped(ty, module, &f.type_parameters)),
            parameters: f
                .parameters
                .iter()
                .map(|p| ApiParameter {
                    name: p.name.clone(),
                    ty: p.ty.to_string(),
                })
                .collect(),
            return_type: f
                .return_type
                .as_ref()
                .map_or("<inferred>".into(), ToString::to_string),
            span: f.span,
            effects: f.effects.as_ref().map_or_else(Vec::new, |d| {
                d.names.iter().map(|n| n.name.clone()).collect()
            }),
            capabilities: f.capabilities.as_ref().map_or_else(Vec::new, |d| {
                d.names.iter().map(|n| n.name.clone()).collect()
            }),
            effects_status: "unchecked".into(),
            capabilities_status: "unchecked".into(),
            requires: f
                .requires
                .iter()
                .map(|e| ApiContract {
                    expression: format_expression(e),
                    status: "unchecked".into(),
                    span: e.span,
                })
                .collect(),
            ensures: f
                .ensures
                .iter()
                .map(|e| ApiContract {
                    expression: format_expression(e),
                    status: "unchecked".into(),
                    span: e.span,
                })
                .collect(),
            foreign: f.foreign.as_ref().map(|b| ApiForeign {
                route: b.route.clone(),
                target: b.target.segments.join("."),
                ownership: b.ownership.clone(),
                runtime: b.runtime.clone(),
                unsafe_call: b.unsafe_call,
                binding_id: None,
                profile: None,
            }),
        })
        .collect();
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    ApiIndex {
        schema_version: 6,
        module: module.name.as_ref().map(|p| p.segments.join(".")),
        functions,
        records: records(module),
        variants: variants(module),
        semantics_status: "syntax-only".into(),
    }
}

fn loop_contracts(function: &cgx2_syntax::Function) -> Vec<ApiLoopContract> {
    let mut contracts = Vec::new();
    cgx2_syntax::visit_stmts(
        &function.body,
        &mut |stmt| {
            if let cgx2_syntax::StmtKind::LoopContract {
                decreases,
                expression,
            } = &stmt.kind
            {
                contracts.push(ApiLoopContract {
                    kind: if *decreases { "decreases" } else { "invariant" }.into(),
                    expression: format_expression(expression),
                    span: stmt.span,
                    status: "unchecked".into(),
                });
            }
        },
        &mut |_| {},
    );
    contracts
}
