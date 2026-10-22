use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cgx2_notebook_format::{
    export_self_contained, import_self_contained, load_asset, parse_notebook, save_asset, Cell,
    CellKind, Notebook, NotebookError, Output,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "cgx2-nb-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

fn image_notebook(digest: &str) -> Notebook {
    Notebook {
        version: 1,
        cells: vec![Cell {
            id: "plot".into(),
            kind: CellKind::Code,
            source: "plot()\n".into(),
            outputs: vec![Output::Asset {
                mime: "image/png".into(),
                ordinal: 3,
                sha256: digest.into(),
            }],
            provenance: None,
        }],
    }
}

#[test]
fn png_asset_is_content_addressed_and_deduplicated() {
    let root = temp_dir();
    let png = b"\x89PNG\r\n\x1a\nbytes";
    let digest = "7cd35944398ce0d57c3c74ef57515a42b7b6f14743dfd08b6cc53cd749c90378";
    assert_eq!(save_asset(&root, png).unwrap(), digest);
    assert_eq!(save_asset(&root, png).unwrap(), digest);
    assert_eq!(load_asset(&root, digest).unwrap(), png);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_and_corrupt_digests_are_rejected() {
    let root = temp_dir();
    for bad in [
        "../escape",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "a",
        "a/../b",
    ] {
        assert!(matches!(
            load_asset(&root, bad),
            Err(NotebookError::InvalidDigest(_))
        ));
    }
    let digest = save_asset(&root, b"valid").unwrap();
    fs::write(root.join(&digest), b"tampered").unwrap();
    assert!(matches!(
        load_asset(&root, &digest),
        Err(NotebookError::HashMismatch { .. })
    ));
    assert!(matches!(
        load_asset(&root, &"0".repeat(64)),
        Err(NotebookError::MissingAsset(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn export_import_reopens_without_original_sidecar() {
    let original = temp_dir();
    let destination = temp_dir();
    let png = b"\x89PNG\r\n\x1a\nbytes";
    let digest = save_asset(&original, png).unwrap();
    let mut notebook = image_notebook(&digest);
    notebook.cells[0].provenance = Some(cgx2_notebook_format::make_provenance(
        &notebook,
        0,
        3,
        Some(3),
    ));
    let exported = export_self_contained(&notebook, &original).unwrap();
    fs::remove_dir_all(original).unwrap();
    let imported = import_self_contained(&exported, &destination).unwrap();
    assert_eq!(imported, notebook);
    assert_eq!(load_asset(&destination, &digest).unwrap(), png);
    assert!(parse_notebook(&exported).is_err());
    fs::remove_dir_all(destination).unwrap();
}

#[test]
fn missing_assets_and_mismatched_embedded_data_fail() {
    let root = temp_dir();
    let digest = "0".repeat(64);
    let notebook = image_notebook(&digest);
    assert!(matches!(
        export_self_contained(&notebook, &root),
        Err(NotebookError::MissingAsset(_))
    ));
    let source = temp_dir();
    let real = save_asset(&source, b"real").unwrap();
    let exported = export_self_contained(&image_notebook(&real), &source).unwrap();
    let corrupted = exported.replace("cmVhbA==", "ZmFrZQ==");
    assert!(matches!(
        import_self_contained(&corrupted, &root),
        Err(NotebookError::HashMismatch { .. })
    ));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(source).unwrap();
}

#[test]
fn notebook_bytes_and_asset_size_limits_are_enforced() {
    let root = temp_dir();
    let big = vec![0; cgx2_notebook_format::MAX_ASSET_BYTES + 1];
    assert!(matches!(
        save_asset(&root, &big),
        Err(NotebookError::SizeLimit { .. })
    ));
    assert!(matches!(
        cgx2_notebook_format::parse_notebook_bytes(&[0xff]),
        Err(NotebookError::InvalidUtf8)
    ));
    assert!(matches!(
        parse_notebook(&"x".repeat(cgx2_notebook_format::MAX_NOTEBOOK_BYTES + 1)),
        Err(NotebookError::SizeLimit { .. })
    ));
    fs::remove_dir_all(root).unwrap();
}
