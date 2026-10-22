use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cgx2_notebook_format::{
    export_self_contained, import_self_contained, make_provenance, open_notebook_bytes,
    parse_self_contained, save_asset, Cell, CellKind, Notebook, NotebookError, Output,
    MAX_ASSET_BYTES, MAX_NOTEBOOK_BYTES,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-portable-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn sample(digest: &str) -> Notebook {
    let mut notebook = Notebook {
        version: 1,
        cells: vec![
            Cell {
                id: "notes".into(),
                kind: CellKind::Markdown,
                source: "# Plot\n".into(),
                outputs: vec![],
                provenance: None,
            },
            Cell {
                id: "seed".into(),
                kind: CellKind::Code,
                source: "let x = 3\n".into(),
                outputs: vec![
                    Output::Text {
                        mime: "text/plain".into(),
                        ordinal: 1,
                        body: "3".into(),
                    },
                    Output::Asset {
                        mime: "image/png".into(),
                        ordinal: 1,
                        sha256: digest.into(),
                    },
                ],
                provenance: None,
            },
            Cell {
                id: "result".into(),
                kind: CellKind::Code,
                source: "x + 4\n".into(),
                outputs: vec![Output::Text {
                    mime: "text/plain".into(),
                    ordinal: 2,
                    body: "7".into(),
                }],
                provenance: None,
            },
        ],
    };
    for (index, ordinal) in [(1, 1), (2, 2)] {
        notebook.cells[index].provenance =
            Some(make_provenance(&notebook, index, ordinal, Some(ordinal)));
    }
    notebook
}

#[test]
fn portable_document_reopens_without_sidecar_and_keeps_exact_observations() {
    let fixture = Fixture::new();
    let root = fixture.path("source.cgnb.assets");
    let png = b"\x89PNG\r\n\x1a\nportable pixels";
    let digest = save_asset(&root, png).unwrap();
    let notebook = sample(&digest);
    let exported = export_self_contained(&notebook, &root).unwrap();
    assert!(exported.contains(&format!(
        "sha256={digest} mime=image/png size={}",
        png.len()
    )));
    fs::remove_dir_all(root).unwrap();

    let opened = open_notebook_bytes(exported.as_bytes(), &fixture.path("missing.assets")).unwrap();
    assert_eq!(opened.notebook, notebook);
    assert_eq!(opened.assets[&digest], png);
    assert!(!fixture.path("missing.assets").exists());

    let restored = import_self_contained(&exported, &fixture.path("restored.assets")).unwrap();
    assert_eq!(restored, notebook);
    assert_eq!(
        fs::read(fixture.path("restored.assets").join(digest)).unwrap(),
        png
    );
}

#[test]
fn corrupt_portable_assets_fail_before_creating_destination() {
    let fixture = Fixture::new();
    let root = fixture.path("source.assets");
    let digest = save_asset(&root, b"image data").unwrap();
    let exported = export_self_contained(&sample(&digest), &root).unwrap();
    let cases = [
        exported.replace("mime=image/png size=", "mime=image/svg+xml size="),
        exported.replace("aW1hZ2UgZGF0YQ==", "c2NyaXB0IGRhdGE="),
        exported.replace(&format!("sha256={digest} mime="), "sha256=../escape mime="),
        format!("{exported}%% embedded sha256={digest} mime=image/png size=10\naW1hZ2UgZGF0YQ==\n"),
        exported.replace("size=10", "size=999999999"),
        exported.replace("aW1hZ2UgZGF0YQ==", &"A".repeat(MAX_NOTEBOOK_BYTES)),
    ];
    for (index, candidate) in cases.into_iter().enumerate() {
        let destination = fixture.path(&format!("out-{index}.assets"));
        assert!(
            import_self_contained(&candidate, &destination).is_err(),
            "{index}"
        );
        assert!(!destination.exists(), "{index}");
    }
    assert!(matches!(
        open_notebook_bytes(&[0xff], &root),
        Err(NotebookError::InvalidUtf8)
    ));
}

#[test]
fn active_asset_mime_and_symlink_escapes_are_rejected() {
    let fixture = Fixture::new();
    let root = fixture.path("assets");
    let digest = save_asset(&root, b"<script>alert(1)</script>").unwrap();
    let mut notebook = sample(&digest);
    if let Output::Asset { mime, .. } = &mut notebook.cells[1].outputs[1] {
        *mime = "text/html".into();
    }
    assert!(matches!(
        export_self_contained(&notebook, &root),
        Err(NotebookError::InvalidOutput(_))
    ));
    let mut notebook = sample(&digest);
    if let Output::Asset { mime, .. } = &mut notebook.cells[1].outputs[1] {
        *mime = "image/svg+xml".into();
    }
    assert!(export_self_contained(&notebook, &root).is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = fixture.path("linked.assets");
        symlink(&root, &link).unwrap();
        assert!(export_self_contained(&sample(&digest), &link).is_err());
        fs::remove_file(root.join(&digest)).unwrap();
        symlink(fixture.path("outside"), root.join(&digest)).unwrap();
        assert!(export_self_contained(&sample(&digest), &root).is_err());
    }
}

#[test]
fn total_and_single_asset_limits_apply_before_encoding() {
    let fixture = Fixture::new();
    let root = fixture.path("assets");
    let large = vec![1; MAX_ASSET_BYTES];
    let first = save_asset(&root, &large).unwrap();
    let second = save_asset(&root, &vec![2; MAX_ASSET_BYTES]).unwrap();
    let mut notebook = sample(&first);
    notebook.cells[1].outputs.push(Output::Asset {
        mime: "image/png".into(),
        ordinal: 1,
        sha256: second,
    });
    assert!(matches!(
        export_self_contained(&notebook, &root),
        Err(NotebookError::SizeLimit {
            what: "all assets",
            ..
        })
    ));
    let tiny = save_asset(&root, b"a").unwrap();
    let text = export_self_contained(&sample(&tiny), &root).unwrap();
    assert!(parse_self_contained(&text.replace("size=1", "size=8388609")).is_err());
}
