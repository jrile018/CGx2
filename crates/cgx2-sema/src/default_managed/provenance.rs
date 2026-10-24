//! Original inputs, never a caller assertion that an unrelated AST is trusted.
use super::{diagnostic, ManagedProgramV1};
use crate::{ImportedSignatures, TypeDiagnostic};
use cgx2_syntax::{Module, Span, TypeParameter};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Additive provenance schema 1. Public values are descriptive inputs, not
/// certificates: constructors and authenticate reparse and recheck every source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedProvenanceV1 {
    Source {
        text: String,
    },
    /// No verified original source is available.
    AstOnly,
    /// (source identity, original UTF-8 bytes), in deterministic caller order.
    /// This authenticates a source snapshot; filesystem/manifest freshness is
    /// additionally enforced by CheckedPackage before publishing its artifacts.
    SourceBundle {
        sources: Vec<(String, String)>,
        entry: String,
    },
}
impl ManagedProvenanceV1 {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Source { .. } => "source-v1",
            Self::AstOnly => "ast-only-v1",
            Self::SourceBundle { .. } => "source-bundle-v1",
        }
    }
    pub fn identity_sha256(&self) -> String {
        let mut hash = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(self.kind().as_bytes());
        match self {
            Self::Source { text } => field(text.as_bytes()),
            Self::AstOnly => {}
            Self::SourceBundle { sources, entry } => {
                field(entry.as_bytes());
                for (name, text) in sources {
                    field(name.as_bytes());
                    field(text.as_bytes());
                }
            }
        }
        format!("{:x}", hash.finalize())
    }
}
pub(super) fn parse_source(source: &str) -> Result<Module, Vec<TypeDiagnostic>> {
    if source.len() > 4 * 1024 * 1024 {
        return Err(vec![diagnostic(
            "managed source exceeds 4 MiB",
            Span { start: 0, end: 0 },
        )]);
    }
    cgx2_syntax::parse_module(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| TypeDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect()
    })
}
fn scope(module: &Module, parameters: &[TypeParameter]) -> Module {
    let mut scope = module.clone();
    scope
        .records
        .retain(|r| !parameters.iter().any(|p| p.name == r.name));
    scope
        .variants
        .retain(|v| !parameters.iter().any(|p| p.name == v.name));
    scope
}
/// Reconstruct a package-style merged module from original sources, retaining
/// direct-import visibility and body-checked transitive behavioral evidence.
/// No caller-provided AST, typed module, callee summary or trusted flag is used.
pub fn compile_source_bundle_v1(
    sources: &[(String, String)],
    entry: &str,
) -> Result<ManagedProgramV1, Vec<TypeDiagnostic>> {
    let merged = source_bundle_module(sources, entry, |module, _| Ok(module.clone()))?;
    super::compile(
        &merged,
        ManagedProvenanceV1::SourceBundle {
            sources: sources.into(),
            entry: entry.into(),
        },
    )
}

pub(crate) fn source_bundle_module(
    sources: &[(String, String)],
    entry: &str,
    prepare: impl Fn(&Module, &[Module]) -> Result<Module, Vec<TypeDiagnostic>>,
) -> Result<Module, Vec<TypeDiagnostic>> {
    let zero = Span { start: 0, end: 0 };
    let fail = |message| vec![diagnostic(message, zero)];
    if sources.is_empty() || sources.len() > 128 {
        return Err(fail("managed source bundle requires 1..128 sources"));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeMap::new();
    let mut modules = Vec::new();
    let mut entry_index = None;
    for (i, (id, text)) in sources.iter().enumerate() {
        if !ids.insert(id) {
            return Err(fail("duplicate managed source identity"));
        }
        let module = parse_source(text)?;
        if id == entry {
            entry_index = Some(i);
        }
        if let Some(name) = &module.name {
            if names.insert(name.segments.join("."), i).is_some() {
                return Err(fail("duplicate managed module identity"));
            }
        } else if sources.len() != 1 {
            return Err(fail("multi-source managed bundle requires named modules"));
        }
        modules.push(module);
    }
    let entry_index = entry_index.ok_or_else(|| fail("missing managed entry source"))?;
    let original_modules = modules;
    let modules = original_modules
        .iter()
        .map(|module| prepare(module, &original_modules))
        .collect::<Result<Vec<_>, _>>()?;
    let mut imported = ImportedSignatures::default();
    imported.modules.extend(names.keys().cloned());
    for module in &modules {
        let Some(path) = &module.name else { continue };
        let name = path.segments.join(".");
        for record in &module.records {
            let id = format!("{name}.{}", record.name);
            if record.public {
                let scope = scope(module, &record.type_parameters);
                let mut record = record.clone();
                record.name = id.clone();
                for field in &mut record.fields {
                    cgx2_syntax::qualify_type(&mut field.ty, &scope);
                }
                imported.public_records.insert(id, record);
            } else {
                imported.private_records.insert(id);
            }
        }
        for variant in &module.variants {
            let id = format!("{name}.{}", variant.name);
            if variant.public {
                let scope = scope(module, &variant.type_parameters);
                let mut variant = variant.clone();
                variant.name = id.clone();
                for case in &mut variant.cases {
                    for payload in &mut case.payloads {
                        cgx2_syntax::qualify_type(payload, &scope);
                    }
                }
                imported.public_variants.insert(id, variant);
            } else {
                imported.private_variants.insert(id);
            }
        }
    }
    for module in &modules {
        if let Some(path) = &module.name {
            let name = path.segments.join(".");
            for (function, signature) in
                crate::collect_public_signatures_with_imports(module, &imported)?
            {
                imported
                    .public_functions
                    .insert(format!("{name}.{function}"), signature);
            }
            for function in module.functions.iter().filter(|f| !f.public) {
                imported
                    .private_functions
                    .insert(format!("{name}.{}", function.name));
            }
        }
        for import in &module.imports {
            if !names.contains_key(&import.segments.join(".")) {
                return Err(vec![diagnostic(
                    "missing managed source dependency",
                    import.span,
                )]);
            }
        }
    }
    let mut checked = BTreeSet::new();
    while checked.len() < modules.len() {
        let mut progress = false;
        for (i, module) in modules.iter().enumerate() {
            if checked.contains(&i)
                || module
                    .imports
                    .iter()
                    .any(|p| !checked.contains(&names[&p.segments.join(".")]))
            {
                continue;
            }
            imported.check_and_record_module(module)?;
            checked.insert(i);
            progress = true;
        }
        if !progress {
            return Err(fail("cyclic managed source dependencies"));
        }
    }
    let entry_module = &original_modules[entry_index];
    let entry_name = entry_module
        .name
        .as_ref()
        .map(|p| p.segments.join("."))
        .unwrap_or_default();
    let mut merged = if modules.len() == 1 && entry_module.name.is_none() {
        entry_module.clone()
    } else {
        crate::merge_modules(&original_modules.iter().collect::<Vec<_>>(), &entry_name)
    };
    merged.span = entry_module.span;
    Ok(merged)
}
