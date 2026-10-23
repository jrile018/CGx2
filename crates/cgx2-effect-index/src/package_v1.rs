//! Additive checked-package advisory API. Imported trust stays in `cgx2-package`.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cgx2_package::{load_checked_package, CheckedPackage, PackageDiagnostic};
use cgx2_syntax::Span;
use serde_json::{json, Value};

use crate::{build_index, CallKind};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub source: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// Manifest-relative UTF-8 source path; spans are half-open UTF-8 byte offsets.
    pub source: PathBuf,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Function(Identity),
    Builtin,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageCall {
    pub target: Option<String>,
    pub location: Location,
    pub resolution: Resolution,
    pub declaration: Option<Location>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEntry {
    pub identity: Identity,
    pub declaration: Location,
    pub public: bool,
    pub effects: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
    pub calls: Vec<PackageCall>,
}

/// A checked snapshot. Serialization rechecks all source and manifest provenance.
#[derive(Clone, Debug)]
pub struct PackageAnalysis {
    checked: CheckedPackage,
    entries: Vec<PackageEntry>,
}

pub fn index_package(root: &Path) -> Result<PackageAnalysis, Vec<PackageDiagnostic>> {
    index_checked_package(&load_checked_package(root)?)
}

pub fn index_checked_package(
    checked: &CheckedPackage,
) -> Result<PackageAnalysis, Vec<PackageDiagnostic>> {
    checked.verify_unchanged()?;
    let mut entries = Vec::new();
    for module in checked.modules() {
        let index = build_index(module.syntax(), Some(module.typed()));
        for entry in index
            .functions
            .iter()
            .chain(std::iter::once(&index.top_level))
        {
            let mut effects = BTreeSet::new();
            let mut capabilities = BTreeSet::new();
            if let Some(summary) = module.typed().effects.get(&entry.name) {
                effects.clone_from(&summary.effects);
                capabilities.clone_from(&summary.capabilities);
            } else {
                effects.extend(entry.direct_effects.iter().map(|e| e.as_str().into()));
            }
            let mut calls = Vec::new();
            for call in &entry.calls {
                let function = call.target.as_ref().and_then(|name| {
                    if call.kind == CallKind::Local {
                        return module
                            .syntax()
                            .functions
                            .iter()
                            .find(|f| f.name == *name)
                            .map(|f| (module, f));
                    }
                    // Exact, public, explicitly imported identity verified by the loader.
                    let record = checked
                        .index()
                        .modules
                        .iter()
                        .find(|m| m.source == module.source())?;
                    if !record
                        .resolved_public_calls
                        .iter()
                        .any(|c| c.name == *name && c.span == call.span)
                    {
                        return None;
                    }
                    checked.modules().iter().find_map(|m| {
                        let prefix = m.syntax().name.as_ref()?.segments.join(".");
                        m.syntax()
                            .functions
                            .iter()
                            .find(|f| f.public && format!("{prefix}.{}", f.name) == *name)
                            .map(|f| (m, f))
                    })
                });
                let (resolution, declaration) = if let Some((owner, function)) = function {
                    (
                        Resolution::Function(Identity {
                            source: owner.source().into(),
                            name: function.name.clone(),
                        }),
                        Some(Location {
                            source: owner.source().into(),
                            span: function.span,
                        }),
                    )
                } else if call.kind == CallKind::Builtin
                    || call
                        .target
                        .as_deref()
                        .is_some_and(|n| n == "range" || n.starts_with("$cgx2_cmp_"))
                {
                    (Resolution::Builtin, None)
                } else {
                    effects.insert("Unknown".into());
                    (Resolution::Unknown, None)
                };
                calls.push(PackageCall {
                    target: call.target.clone(),
                    location: Location {
                        source: module.source().into(),
                        span: call.span,
                    },
                    resolution,
                    declaration,
                });
            }
            // The standalone visitor marks imports unknown. Only resolved checked edges
            // can remove that marker for script entries; function summaries are typed.
            if entry.name == "<top-level>" {
                if !calls.iter().any(|c| c.resolution == Resolution::Unknown) {
                    effects.remove("Unknown");
                }
                for call in &calls {
                    if let Resolution::Function(id) = &call.resolution {
                        let target = checked
                            .modules()
                            .iter()
                            .find(|m| m.source() == id.source)
                            .expect("checked target");
                        let summary = &target.typed().effects[&id.name];
                        effects.extend(summary.effects.iter().cloned());
                        capabilities.extend(summary.capabilities.iter().cloned());
                    } else {
                        match call.target.as_deref() {
                            Some("arg" | "arg_count") => {
                                capabilities.insert("ArgsRead".into());
                            }
                            Some("memory.collect") => {
                                capabilities.insert("MemoryCollect".into());
                            }
                            _ => {}
                        }
                    }
                }
            }
            entries.push(PackageEntry {
                identity: Identity {
                    source: module.source().into(),
                    name: entry.name.clone(),
                },
                declaration: Location {
                    source: module.source().into(),
                    span: entry.span,
                },
                public: entry.public,
                effects,
                capabilities,
                calls,
            });
        }
    }
    entries.sort_by(|a, b| a.identity.cmp(&b.identity));
    checked.verify_unchanged()?;
    Ok(PackageAnalysis {
        checked: checked.clone(),
        entries,
    })
}

impl PackageAnalysis {
    pub fn checked(&self) -> &CheckedPackage {
        &self.checked
    }
    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }
    pub fn entry(&self, identity: &Identity) -> Option<&PackageEntry> {
        self.entries.iter().find(|e| e.identity == *identity)
    }
    pub fn call(&self, source: &Path, span: Span) -> Option<&PackageCall> {
        self.entries
            .iter()
            .filter(|e| e.identity.source == source)
            .flat_map(|e| &e.calls)
            .find(|c| c.location.span == span)
    }
    pub fn verify_unchanged(&self) -> Result<(), Vec<PackageDiagnostic>> {
        self.checked.verify_unchanged()
    }
    pub fn to_json(&self) -> Result<String, Vec<PackageDiagnostic>> {
        self.verify_unchanged()?;
        let entries: Vec<_> = self.entries.iter().map(|e| json!({
            "identity":identity_json(&e.identity), "declaration": location_json(&e.declaration),
            "public":e.public,"effects":e.effects,"capabilities":e.capabilities,
            "calls":e.calls.iter().map(|c| json!({"target":c.target,"location":location_json(&c.location),
                "resolution":match c.resolution { Resolution::Function(_) => "function", Resolution::Builtin => "builtin", Resolution::Unknown => "unknown" },
                "identity":if let Resolution::Function(id)=&c.resolution {identity_json(id)} else {Value::Null},
                "declaration":c.declaration.as_ref().map(location_json)})).collect::<Vec<_>>()
        })).collect();
        let provenance: Vec<_> = self
            .checked
            .modules()
            .iter()
            .map(|m| json!({"source":m.source(),"sha256":m.source_sha256()}))
            .collect();
        Ok(json!({"schema_version":1,"analysis":"checked_package_effects_v1","advisory":true,"span_unit":"utf8_byte_offset","manifest_sha256":self.checked.manifest_sha256(),"provenance":provenance,"entries":entries}).to_string())
    }
    pub fn to_text(&self) -> Result<String, Vec<PackageDiagnostic>> {
        self.verify_unchanged()?;
        let mut out = String::from("checked package advisory effects/capabilities v1\n");
        for e in &self.entries {
            out.push_str(&format!(
                "{}:{} [{}..{}): effects {:?}; capabilities {:?}\n",
                e.identity.source.display(),
                e.identity.name,
                e.declaration.span.start,
                e.declaration.span.end,
                e.effects,
                e.capabilities
            ));
        }
        Ok(out)
    }
}

pub fn location_json(location: &Location) -> Value {
    json!({"source":location.source,"span":{"start":location.span.start,"end":location.span.end}})
}
fn identity_json(identity: &Identity) -> Value {
    json!({"source":identity.source,"name":identity.name})
}
