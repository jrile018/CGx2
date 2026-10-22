use cgx2_sema::{check_module, check_module_with_imports, ImportedSignatures, TypedModule};
use cgx2_syntax::{format_expression, Module, Span};

use crate::IndexError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiIndex {
    pub schema_version: u32,
    pub module: Option<String>,
    pub functions: Vec<ApiFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiFunction {
    pub name: String,
    pub parameters: Vec<ApiParameter>,
    pub return_type: String,
    pub span: Span,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub effects_status: String,
    pub capabilities_status: String,
    pub requires: Vec<ApiContract>,
    pub ensures: Vec<ApiContract>,
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
        schema_version: if module.functions.iter().any(|function| {
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
    })
}

impl ApiIndex {
    /// Look up an exact public function name.
    pub fn find(&self, name: &str) -> Option<&ApiFunction> {
        self.functions.iter().find(|function| function.name == name)
    }
}
