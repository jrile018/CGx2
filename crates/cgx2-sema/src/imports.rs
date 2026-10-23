//! Body-checked import evidence, owned by the package snapshot rather than a cache.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cgx2_syntax::{Module, Record};

use crate::{
    check_module_with_imports, EffectSummary, FunctionSignature, ImportedSignatures,
    TypeDiagnostic, TypedModule,
};

/// Only `ImportedSignatures::check_and_record_module` can publish summaries.
/// Cloning retains immutable evidence; an empty/default value trusts nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckedImportedEffects(BTreeMap<String, Arc<CheckedModuleEffects>>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckedModuleEffects {
    source: Module,
    effects: BTreeMap<String, EffectSummary>,
    declarations: Declarations,
    dependencies: BTreeMap<String, Arc<CheckedModuleEffects>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Declarations {
    modules: BTreeSet<String>,
    public_functions: BTreeMap<String, FunctionSignature>,
    private_functions: BTreeSet<String>,
    public_records: BTreeMap<String, Record>,
    private_records: BTreeSet<String>,
}

impl Declarations {
    fn capture(imports: &ImportedSignatures) -> Self {
        Self {
            modules: imports.modules.clone(),
            public_functions: imports.public_functions.clone(),
            private_functions: imports.private_functions.clone(),
            public_records: imports.public_records.clone(),
            private_records: imports.private_records.clone(),
        }
    }
}

impl ImportedSignatures {
    /// Check a body against the current environment and publish its public effects
    /// only on success. Package loaders call this in dependency-first order, after
    /// collecting all declarations. Unnamed standalone modules publish nothing.
    /// Changing declarations or replacing/removing dependency evidence makes older
    /// summaries conservative again; callers must be checked against the new snapshot.
    pub fn check_and_record_module(
        &mut self,
        module: &Module,
    ) -> Result<TypedModule, Vec<TypeDiagnostic>> {
        // A failed refresh must not leave an older body trusted under this identity.
        if let Some(path) = &module.name {
            self.checked_effects.0.remove(&path.segments.join("."));
        }
        let typed = check_module_with_imports(module, self)?;
        let Some(path) = &module.name else {
            return Ok(typed);
        };
        let name = path.segments.join(".");
        let mut effects = BTreeMap::new();
        for function in module.functions.iter().filter(|function| function.public) {
            let identity = format!("{name}.{}", function.name);
            if !self.modules.contains(&name)
                || self.private_functions.contains(&identity)
                || self.public_functions.get(&identity) != typed.functions.get(&function.name)
            {
                return Err(vec![TypeDiagnostic::new(
                    "CGT010",
                    format!("checked function `{identity}` does not match imported declarations"),
                    function.span,
                )]);
            }
            effects.insert(identity, typed.effects[&function.name].clone());
        }
        let dependencies = module
            .imports
            .iter()
            .filter_map(|import| {
                let name = import.segments.join(".");
                self.checked_effects
                    .0
                    .get(&name)
                    .map(|summary| (name, Arc::clone(summary)))
            })
            .collect();
        self.checked_effects.0.insert(
            name,
            Arc::new(CheckedModuleEffects {
                source: module.clone(),
                effects,
                declarations: Declarations::capture(self),
                dependencies,
            }),
        );
        Ok(typed)
    }

    pub(crate) fn verified_effects(&self, module: &Module) -> BTreeMap<String, EffectSummary> {
        fn valid(
            summary: &CheckedModuleEffects,
            imports: &ImportedSignatures,
            declarations: &Declarations,
            depth: usize,
            validated: &mut BTreeSet<String>,
        ) -> bool {
            let name = summary
                .source
                .name
                .as_ref()
                .expect("named checked module")
                .segments
                .join(".");
            if validated.contains(&name) {
                return true;
            }
            let valid = depth <= 256
                && summary.declarations == *declarations
                && summary.dependencies.iter().all(|(name, dependency)| {
                    imports.checked_effects.0.get(name).is_some_and(|current| {
                        Arc::ptr_eq(current, dependency)
                            && valid(dependency, imports, declarations, depth + 1, validated)
                    })
                });
            if valid {
                validated.insert(name);
            }
            valid
        }
        let declarations = Declarations::capture(self);
        let mut effects = BTreeMap::new();
        let mut validated = BTreeSet::new();
        for import in &module.imports {
            let name = import.segments.join(".");
            if let Some(summary) = self.checked_effects.0.get(&name) {
                if valid(summary, self, &declarations, 0, &mut validated) {
                    effects.extend(summary.effects.clone());
                }
            }
        }
        effects
    }
}
