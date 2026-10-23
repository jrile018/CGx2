use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cgx2_notebook_format::{
    export_document, export_self_contained, image_mime, import_self_contained, open_notebook_bytes,
    parse_self_contained, save_asset, write_notebook, Cell, CellKind, Notebook, NotebookError,
    MAX_ASSET_BYTES, MAX_DISTINCT_ASSETS,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-markdown-images-{}-{}",
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

fn notebook(source: String) -> Notebook {
    Notebook {
        version: 1,
        cells: vec![Cell {
            id: "figures".into(),
            kind: CellKind::Markdown,
            source,
            outputs: vec![],
            provenance: None,
        }],
    }
}

#[test]
fn two_markdown_images_roundtrip_without_original_sidecar() {
    let fixture = Fixture::new();
    let root = fixture.path("input.cgnb.assets");
    let png = b"\x89PNG\r\n\x1a\nfigure one";
    let gif = b"GIF89afigure two";
    let first = save_asset(&root, png).unwrap();
    let second = save_asset(&root, gif).unwrap();
    let notebook = notebook(format!(
        "![First](asset:sha256:{first})\n![Second](asset:sha256:{second})\n"
    ));
    let native = write_notebook(&notebook).unwrap();
    let opened = open_notebook_bytes(native.as_bytes(), &root).unwrap();
    assert_eq!(opened.assets.len(), 2);
    let portable = export_self_contained(&notebook, &root).unwrap();
    assert!(portable.contains(&format!("sha256={first} mime=image/png")));
    assert!(portable.contains(&format!("sha256={second} mime=image/gif")));
    fs::remove_dir_all(root).unwrap();
    let reopened = open_notebook_bytes(portable.as_bytes(), &fixture.path("gone.assets")).unwrap();
    assert_eq!(reopened, opened);
    assert_eq!(parse_self_contained(&portable).unwrap(), opened);
    let imported_root = fixture.path("imported.assets");
    assert_eq!(
        import_self_contained(&portable, &imported_root).unwrap(),
        notebook
    );
    assert_eq!(fs::read(imported_root.join(first)).unwrap(), png);
    assert_eq!(fs::read(imported_root.join(second)).unwrap(), gif);
}

#[test]
fn malformed_missing_extra_and_corrupt_images_fail() {
    let fixture = Fixture::new();
    let root = fixture.path("assets");
    let png = b"\x89PNG\r\n\x1a\nfigure";
    let digest = save_asset(&root, png).unwrap();
    for bad in ["short", &"A".repeat(64), &"0".repeat(65)] {
        assert!(matches!(
            write_notebook(&notebook(format!("![bad](asset:sha256:{bad})\n"))),
            Err(NotebookError::InvalidDigest(_))
        ));
    }
    let missing = notebook(format!("![lost](asset:sha256:{})\n", "0".repeat(64)));
    assert!(matches!(
        export_self_contained(&missing, &root),
        Err(NotebookError::MissingAsset(_))
    ));
    let valid = notebook(format!("![plot](asset:sha256:{digest})\n"));
    let native = write_notebook(&valid).unwrap();
    let portable = export_self_contained(&valid, &root).unwrap();
    let extra = save_asset(&root, b"GIF89aextra").unwrap();
    assert!(open_notebook_bytes(native.as_bytes(), &root).is_err());
    assert!(export_self_contained(&valid, &root).is_err());
    assert!(export_document(
        &valid,
        &[
            (digest.clone(), png.to_vec()),
            (extra, b"GIF89aextra".to_vec())
        ]
        .into()
    )
    .is_err());
    fs::remove_dir_all(&root).unwrap();
    fs::create_dir(&root).unwrap();
    fs::write(root.join(&digest), b"tampered").unwrap();
    assert!(matches!(
        open_notebook_bytes(native.as_bytes(), &root),
        Err(NotebookError::HashMismatch { .. })
    ));
    let appended = format!("{portable}%% embedded sha256={digest} mime=image/png size=1\nYQ==\n");
    assert!(parse_self_contained(&appended).is_err());
    let blocked = fixture.path("blocked.assets");
    save_asset(&blocked, b"GIF89aextra").unwrap();
    assert!(import_self_contained(&portable, &blocked).is_err());
    assert!(!blocked.join(&digest).exists());
    assert!(open_notebook_bytes(native.as_bytes(), &fixture.path("missing-root")).is_err());
}

#[test]
fn only_image_signatures_are_accepted_and_limits_apply() {
    let fixture = Fixture::new();
    let root = fixture.path("assets");
    let script = save_asset(&root, b"<script>alert(1)</script>").unwrap();
    let unsafe_notebook = notebook(format!("![unsafe](asset:sha256:{script})\n"));
    assert!(matches!(
        export_self_contained(&unsafe_notebook, &root),
        Err(NotebookError::InvalidOutput(_))
    ));
    assert_eq!(image_mime(b"\xff\xd8\xffjpeg"), Some("image/jpeg"));
    assert_eq!(image_mime(b"RIFF0000WEBPdata"), Some("image/webp"));
    assert_eq!(image_mime(b"<svg></svg>"), None);
    let too_many = (0..=MAX_DISTINCT_ASSETS)
        .map(|n| format!("![{n}](asset:sha256:{n:064x})\n"))
        .collect();
    assert!(matches!(
        write_notebook(&notebook(too_many)),
        Err(NotebookError::SizeLimit {
            what: "asset count",
            ..
        })
    ));
    assert!(matches!(
        save_asset(&root, &vec![0; MAX_ASSET_BYTES + 1]),
        Err(NotebookError::SizeLimit { what: "asset", .. })
    ));
    let ordinary =
        notebook("![local](../figure.png)\n![remote](https://example.test/a.png)\n".into());
    assert!(open_notebook_bytes(
        write_notebook(&ordinary).unwrap().as_bytes(),
        &fixture.path("none")
    )
    .is_ok());
}

#[cfg(unix)]
#[test]
fn symlinked_sidecar_and_asset_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let root = fixture.path("assets");
    let digest = save_asset(&root, b"GIF89afigure").unwrap();
    let native = write_notebook(&notebook(format!("![x](asset:sha256:{digest})\n"))).unwrap();
    let link = fixture.path("linked.assets");
    symlink(&root, &link).unwrap();
    assert!(open_notebook_bytes(native.as_bytes(), &link).is_err());
    fs::remove_file(root.join(&digest)).unwrap();
    symlink(fixture.path("outside"), root.join(&digest)).unwrap();
    assert!(open_notebook_bytes(native.as_bytes(), &root).is_err());
}
