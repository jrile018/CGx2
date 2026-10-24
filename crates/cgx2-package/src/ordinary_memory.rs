//! A distinct package certificate: erased memory types are never exposed as an
//! ordinary CheckedPackage or passed to consumers that only understand v1.
use super::*;
use cgx2_sema::ordinary_memory::MemoryProgramV1;

#[derive(Clone, Debug)]
pub struct CheckedMemoryPackageV1 {
    root: PathBuf,
    manifest_canonical: PathBuf,
    manifest_sha256: String,
    entry: PathBuf,
    binding_lock: Option<PathBuf>,
    sources: Vec<ParsedSource>,
    program: MemoryProgramV1,
}
impl CheckedMemoryPackageV1 {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn entry(&self) -> &Path {
        &self.entry
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn binding_lock(&self) -> Option<&Path> {
        self.binding_lock.as_deref()
    }
    pub fn source_hashes(&self) -> Vec<(PathBuf, String)> {
        self.sources
            .iter()
            .map(|s| (s.source.clone(), s.sha256.clone()))
            .collect()
    }
    pub fn input_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.manifest_canonical.clone()];
        paths.extend(self.sources.iter().map(|s| s.canonical.clone()));
        paths.extend(self.binding_lock.as_ref().map(|p| self.root.join(p)));
        paths
    }
    pub fn source_for_function(&self, name: &str) -> Option<&Path> {
        let name = name.split('<').next().unwrap_or(name);
        self.sources
            .iter()
            .find(|s| {
                s.module.functions.iter().any(|f| {
                    let identity = if f.name == "main" && s.source == self.entry {
                        "main".into()
                    } else {
                        s.name
                            .as_ref()
                            .map_or_else(|| f.name.clone(), |n| format!("{n}.{}", f.name))
                    };
                    identity == name
                })
            })
            .map(|s| s.source.as_path())
    }
    /// Check both exact contents and canonical identities, including symlink
    /// retargets. Call again immediately before artifact publication.
    pub fn verify_unchanged(&self) -> Result<(), Vec<PackageDiagnostic>> {
        let fail = |path: &Path| {
            vec![PackageDiagnostic::new(
                "CGP018",
                "package input changed after memory validation",
                path,
                None,
            )]
        };
        let manifest = self.root.join("cgx2.toml");
        let canonical = canonicalize_contained(&manifest, &self.root).map_err(|d| vec![d])?;
        if canonical != self.manifest_canonical
            || digest(&read_limited(&canonical, &manifest, MAX_MANIFEST_BYTES)?)
                != self.manifest_sha256
        {
            return Err(fail(&manifest));
        }
        for source in &self.sources {
            let listed = self.root.join(&source.source);
            let canonical = canonicalize_contained(&listed, &self.root).map_err(|d| vec![d])?;
            if canonical != source.canonical
                || digest(&read_limited(&canonical, &listed, MAX_SOURCE_BYTES)?) != source.sha256
            {
                return Err(fail(&listed));
            }
        }
        Ok(())
    }
    pub fn memory_program_v1(&self) -> Result<MemoryProgramV1, Vec<PackageDiagnostic>> {
        self.verify_unchanged()?;
        self.program
            .authenticate()
            .map_err(|ds| self.diagnostics(ds))?;
        Ok(self.program.clone())
    }
    fn diagnostics(&self, ds: Vec<cgx2_sema::TypeDiagnostic>) -> Vec<PackageDiagnostic> {
        ds.into_iter()
            .map(|d| PackageDiagnostic::new(d.code, d.message, &self.entry, Some(d.span)))
            .collect()
    }
}

/// None delegates to the existing package loader. Parsing, containment and path
/// limits use the same reader as ordinary packages; builtin identity is resolved
/// again against checked dependency declarations by the source-bundle producer.
pub fn try_load_checked_memory_package_v1(
    root: &Path,
) -> Result<Option<CheckedMemoryPackageV1>, Vec<PackageDiagnostic>> {
    let inputs = read_sources(root)?;
    if !inputs
        .parsed_sources
        .iter()
        .any(|s| cgx2_sema::ordinary_memory::profile(&s.module))
    {
        return Ok(None);
    }
    let entry = PathBuf::from(&inputs.manifest.entry.value);
    let sources = inputs
        .parsed_sources
        .iter()
        .map(|s| (s.source.to_string_lossy().into_owned(), s.text.clone()))
        .collect::<Vec<_>>();
    let program =
        cgx2_sema::ordinary_memory::compile_source_bundle_v1(&sources, &entry.to_string_lossy())
            .map_err(|ds| {
                ds.into_iter()
                    .map(|d| PackageDiagnostic::new(d.code, d.message, &entry, Some(d.span)))
                    .collect::<Vec<_>>()
            })?;
    if program.runtime().operations.is_empty() {
        return Ok(None);
    }
    let result = CheckedMemoryPackageV1 {
        root: inputs.canonical_root,
        manifest_canonical: inputs.canonical_manifest,
        manifest_sha256: inputs.manifest_sha256,
        entry,
        binding_lock: inputs.manifest.binding_lock,
        sources: inputs.parsed_sources,
        program,
    };
    result.verify_unchanged()?;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "ordinary-memory-package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::write(root.join("cgx2.toml"), "schema_version = 1\nname = \"memory\"\nentry = \"main.cg\"\nsources = [\"main.cg\", \"lib.cg\"]\n").unwrap();
            fs::write(
                root.join("lib.cg"),
                "module app.lib\npub fn answer() -> i64:\n    42\n",
            )
            .unwrap();
            fs::write(root.join("main.cg"), "module app.main\nimport app.lib\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            app.lib.answer()\n        Result.Err(error):\n            -1\n").unwrap();
            Self(root)
        }
        fn load(&self) -> CheckedMemoryPackageV1 {
            try_load_checked_memory_package_v1(&self.0)
                .unwrap()
                .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn source_bundle_and_legacy_consumer_gate_are_distinct() {
        let f = Fixture::new();
        let package = f.load();
        let program = package.memory_program_v1().unwrap();
        assert_eq!(
            program.execute(&[]).unwrap().result.unwrap().to_string(),
            "42"
        );
        assert_eq!(program.provenance_v1().kind(), "source-bundle-v1");
        assert_eq!(
            package.source_for_function("app.lib.answer"),
            Some(Path::new("lib.cg"))
        );
        assert_eq!(
            package.source_for_function("main"),
            Some(Path::new("main.cg"))
        );
        assert_eq!(package.source_hashes().len(), 2);
        assert!(load_checked_package(&f.0).is_err());
    }
    #[test]
    fn changed_private_body_and_manifest_invalidate_certificate() {
        let f = Fixture::new();
        let package = f.load();
        let lib = f.0.join("lib.cg");
        let original = fs::read(&lib).unwrap();
        fs::write(&lib, "module app.lib\npub fn answer() -> i64:\n    41\n").unwrap();
        assert_eq!(package.memory_program_v1().unwrap_err()[0].code, "CGP018");
        let changed = f.load();
        assert_ne!(
            package.program.analysis_sha256(),
            changed.program.analysis_sha256()
        );
        fs::write(&lib, original).unwrap();
        let manifest = f.0.join("cgx2.toml");
        let mut bytes = fs::read(&manifest).unwrap();
        bytes.push(b'\n');
        fs::write(manifest, bytes).unwrap();
        assert_eq!(package.verify_unchanged().unwrap_err()[0].code, "CGP018");
    }
    #[test]
    fn unused_private_body_is_checked() {
        let f = Fixture::new();
        fs::write(
            f.0.join("lib.cg"),
            "module app.lib\npub fn answer() -> i64:\n    42\nfn unused() -> i64:\n    true\n",
        )
        .unwrap();
        assert!(try_load_checked_memory_package_v1(&f.0).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn equal_content_symlink_retarget_is_stale() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        fs::rename(f.0.join("lib.cg"), f.0.join("lib-a.cg")).unwrap();
        fs::copy(f.0.join("lib-a.cg"), f.0.join("lib-b.cg")).unwrap();
        symlink("lib-a.cg", f.0.join("lib.cg")).unwrap();
        let package = f.load();
        fs::remove_file(f.0.join("lib.cg")).unwrap();
        symlink("lib-b.cg", f.0.join("lib.cg")).unwrap();
        assert_eq!(package.verify_unchanged().unwrap_err()[0].code, "CGP018");
    }
}
