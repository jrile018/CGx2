use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_notebook_format::{parse_notebook_bytes, save_asset, Output as CellOutput};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-notebook-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, body: &str) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, body).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2-notebook"))
        .args(args)
        .output()
        .unwrap()
}

fn run_str(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2-notebook"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const INPUT: &str = "#!cgnb 1\n%% markdown id=notes\n# Reference: figure.png\n%% code id=seed\nlet x = 3\n%% output id=seed mime=text/plain ordinal=9\nstale\n%% code id=result\nx + 4\n%% output id=result mime=text/plain ordinal=10\nstale\n";

#[test]
fn run_saves_fresh_reopenable_output_without_touching_input() {
    let fixture = Fixture::new();
    let input = fixture.write("source.cgnb", INPUT);
    let output = fixture.path("executed.cgnb");
    let result = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(fs::read_to_string(&input).unwrap(), INPUT);
    let saved = fs::read(&output).unwrap();
    let notebook = parse_notebook_bytes(&saved).unwrap();
    assert_eq!(notebook.cells[0].source, "# Reference: figure.png\n");
    assert_eq!(notebook.cells[1].id, "seed");
    assert_eq!(notebook.cells[2].id, "result");
    assert!(!String::from_utf8_lossy(&saved).contains("stale"));
    assert!(matches!(
        &notebook.cells[2].outputs[..],
        [CellOutput::Text { mime, ordinal: 1, body }]
            if mime == "text/plain" && body.trim() == "7"
    ));
    assert!(notebook.cells[2].provenance.is_some());
    let inspected = run(&[Path::new("inspect"), &output]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let text = String::from_utf8_lossy(&inspected.stdout);
    assert!(
        text.contains("result\tcode\tcached\tschema=1\tordinal="),
        "{text}"
    );
    assert!(text.contains("source_sha256="), "{text}");
}

#[test]
fn inspect_is_read_only_and_does_not_execute() {
    let fixture = Fixture::new();
    let input = fixture.write("source.cgnb", INPUT);
    let result = run(&[Path::new("inspect"), &input]);
    assert!(result.status.success(), "{}", stderr(&result));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("notes\tmarkdown"), "{stdout}");
    assert!(stdout.contains("seed\tcode\tcached"), "{stdout}");
    assert!(stdout.contains("result\tcode\tcached"), "{stdout}");
    assert_eq!(fs::read_to_string(input).unwrap(), INPUT);
}

#[test]
fn collisions_require_force_and_failure_preserves_prior_output() {
    let fixture = Fixture::new();
    let input = fixture.write("source.cgnb", INPUT);
    let output = fixture.write("existing.cgnb", "prior");
    let args = [Path::new("run"), &input, Path::new("--out"), &output];
    let failed = run(&args);
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("--force"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "prior");
    let forced = run(&[
        Path::new("run"),
        &input,
        Path::new("--out"),
        &output,
        Path::new("--force"),
    ]);
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert!(fs::read_to_string(output).unwrap().contains("ordinal=1"));
    assert_eq!(fs::read_to_string(input).unwrap(), INPUT);
}

#[test]
fn same_path_needs_force_and_can_be_replaced_after_success() {
    let fixture = Fixture::new();
    let input = fixture.write("source.cgnb", INPUT);
    let failed = run(&[Path::new("run"), &input, Path::new("--out"), &input]);
    assert!(!failed.status.success());
    assert_eq!(fs::read_to_string(&input).unwrap(), INPUT);
    let forced = run(&[
        Path::new("run"),
        &input,
        Path::new("--out"),
        &input,
        Path::new("--force"),
    ]);
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert!(fs::read_to_string(input).unwrap().contains("ordinal=1\n7"));
}

#[test]
fn failed_cells_and_io_errors_leave_no_successful_output() {
    let fixture = Fixture::new();
    let bad = fixture.write("bad.cgnb", "#!cgnb 1\n%% code id=broken\nmissing + 1\n");
    let output = fixture.write("prior.cgnb", "prior");
    let failed = run(&[
        Path::new("run"),
        &bad,
        Path::new("--out"),
        &output,
        Path::new("--force"),
    ]);
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("broken"));
    assert!(stderr(&failed).contains("CGNK"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "prior");

    let input = fixture.write("source.cgnb", INPUT);
    let blocked = fixture.write("blocked", "not a directory");
    let destination = blocked.join("output.cgnb");
    let io = run(&[Path::new("run"), &input, Path::new("--out"), &destination]);
    assert!(!io.status.success());
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(input).unwrap(), INPUT);
}

#[test]
fn unsupported_cell_and_missing_asset_fail_without_replacing_output() {
    let fixture = Fixture::new();
    let output = fixture.write("prior.cgnb", "prior");
    let unsupported = fixture.write(
        "unsupported.cgnb",
        "#!cgnb 1\n%% code id=unsupported\nfn double(x: i64) -> i64:\n    x * 2\n",
    );
    let failed = run(&[
        Path::new("run"),
        &unsupported,
        Path::new("--out"),
        &output,
        Path::new("--force"),
    ]);
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("unsupported"));
    assert!(stderr(&failed).contains("CGNK001"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "prior");

    let digest = "0".repeat(64);
    let missing = fixture.write(
        "missing.cgnb",
        &format!("#!cgnb 1\n%% code id=seed\nlet x = 3\n%% asset id=seed mime=image/png ordinal=1 sha256={digest}\n"),
    );
    let failed = run(&[
        Path::new("run"),
        &missing,
        Path::new("--out"),
        &output,
        Path::new("--force"),
    ]);
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("missing asset"));
    assert_eq!(fs::read_to_string(output).unwrap(), "prior");
}

#[test]
fn valid_stale_asset_is_checked_then_dropped_from_clean_code_output() {
    let fixture = Fixture::new();
    let sidecar = fixture.path("source.cgnb.assets");
    let digest = save_asset(&sidecar, b"old image").unwrap();
    let input = fixture.write(
        "source.cgnb",
        &format!("#!cgnb 1\n%% markdown id=notes\n![old](source.cgnb.assets/{digest})\n%% code id=seed\nlet x = 3\n%% asset id=seed mime=image/png ordinal=2 sha256={digest}\n%% code id=result\nx + 4\n"),
    );
    let output = fixture.path("fresh.cgnb");
    let result = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    let saved = parse_notebook_bytes(&fs::read(output).unwrap()).unwrap();
    assert!(saved.cells[0].source.contains(&digest));
    assert!(saved.cells[1]
        .outputs
        .iter()
        .all(|value| matches!(value, CellOutput::Text { mime, .. } if mime == "text/plain")));
    assert_eq!(fs::read(sidecar.join(digest)).unwrap(), b"old image");
}

#[test]
fn portable_export_import_preserves_provenance_and_rejects_corrupt_record() {
    let fixture = Fixture::new();
    let input = fixture.path("source.cgnb");
    let digest = save_asset(&fixture.path("source.cgnb.assets"), b"image bytes").unwrap();
    let mut notebook = cgx2_notebook_format::parse_notebook(&format!(
        "#!cgnb 1\n%% code id=plot\nplot()\n%% asset id=plot mime=image/png ordinal=2 sha256={digest}\n"
    )).unwrap();
    notebook.cells[0].provenance = Some(cgx2_notebook_format::make_provenance(
        &notebook,
        0,
        4,
        Some(2),
    ));
    fs::write(
        &input,
        cgx2_notebook_format::write_notebook(&notebook).unwrap(),
    )
    .unwrap();
    let portable = fixture.path("portable.cgnb");
    let exported = run(&[Path::new("export"), &input, Path::new("--out"), &portable]);
    assert!(exported.status.success(), "{}", stderr(&exported));
    let imported = fixture.path("imported.cgnb");
    let result = run(&[
        Path::new("import"),
        &portable,
        Path::new("--out"),
        &imported,
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(
        parse_notebook_bytes(&fs::read(&imported).unwrap()).unwrap(),
        notebook
    );
    assert_eq!(
        fs::read(fixture.path("imported.cgnb.assets").join(&digest)).unwrap(),
        b"image bytes"
    );
    let corrupt = fixture.write(
        "corrupt.cgnb",
        &fs::read_to_string(portable)
            .unwrap()
            .replace("source_sha256=", "source_sha256=BAD"),
    );
    let rejected = run(&[
        Path::new("import"),
        &corrupt,
        Path::new("--out"),
        &fixture.path("rejected.cgnb"),
    ]);
    assert!(!rejected.status.success());
    assert!(stderr(&rejected).contains("provenance"));
}

#[test]
fn portable_file_inspects_and_runs_without_sidecars_or_source_mutation() {
    let fixture = Fixture::new();
    let root = fixture.path("source.cgnb.assets");
    let digest = save_asset(&root, b"\x89PNG\r\n\x1a\nimage").unwrap();
    let original = format!(
        "#!cgnb 1\n%% markdown id=notes\n# A plot\n%% code id=seed\nlet x = 3\n%% output id=seed mime=text/plain ordinal=1\n3\n%% asset id=seed mime=image/png ordinal=1 sha256={digest}\n%% code id=result\nx + 4\n%% output id=result mime=text/plain ordinal=2\n7\n"
    );
    let source = fixture.write("source.cgnb", &original);
    let portable = fixture.path("portable.cgnb");
    let exported = run(&[Path::new("export"), &source, Path::new("--out"), &portable]);
    assert!(exported.status.success(), "{}", stderr(&exported));
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
    fs::remove_dir_all(root).unwrap();

    let inspected = run(&[Path::new("inspect"), &portable]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let listing = String::from_utf8(inspected.stdout).unwrap();
    assert!(listing.contains("seed\tcode\tcached"));
    assert!(listing.contains("result\tcode\tcached"));

    let executed = fixture.path("executed.cgnb");
    let fresh = run(&[Path::new("run"), &portable, Path::new("--out"), &executed]);
    assert!(fresh.status.success(), "{}", stderr(&fresh));
    let output = parse_notebook_bytes(&fs::read(&executed).unwrap()).unwrap();
    assert_eq!(output.cells[0].source, "# A plot\n");
    assert_eq!(output.cells[2].source, "x + 4\n");
    assert!(output.cells[2].provenance.is_some());
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
    assert!(!fixture.path("portable.cgnb.assets").exists());

    let collision = run(&[Path::new("export"), &source, Path::new("--out"), &portable]);
    assert!(!collision.status.success());
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
}

#[test]
fn rejected_import_preserves_existing_notebook_and_sidecars() {
    let fixture = Fixture::new();
    let digest = save_asset(&fixture.path("source.cgnb.assets"), b"new image").unwrap();
    let input = fixture.write(
        "source.cgnb",
        &format!("#!cgnb 1\n%% code id=plot\n1\n%% asset id=plot mime=image/png ordinal=1 sha256={digest}\n"),
    );
    let portable = fixture.path("portable.cgnb");
    assert!(
        run(&[Path::new("export"), &input, Path::new("--out"), &portable])
            .status
            .success()
    );
    let existing = fixture.write("destination.cgnb", "#!cgnb 1\n%% code id=old\n2\n");
    let old_digest = save_asset(&fixture.path("destination.cgnb.assets"), b"old image").unwrap();
    let before = fs::read(&existing).unwrap();
    let bad = fixture.write(
        "bad.cgnb",
        &fs::read_to_string(&portable)
            .unwrap()
            .replace("mime=image/png size=", "mime=image/svg+xml size="),
    );
    let rejected = run(&[
        Path::new("import"),
        &bad,
        Path::new("--out"),
        &existing,
        Path::new("--force"),
    ]);
    assert!(!rejected.status.success());
    assert_eq!(fs::read(&existing).unwrap(), before);
    assert_eq!(
        fs::read(fixture.path("destination.cgnb.assets").join(&old_digest)).unwrap(),
        b"old image"
    );

    let imported = run(&[
        Path::new("import"),
        &portable,
        Path::new("--out"),
        &existing,
        Path::new("--force"),
    ]);
    assert!(imported.status.success(), "{}", stderr(&imported));
    assert_eq!(
        fs::read(fixture.path("destination.cgnb.assets").join(&digest)).unwrap(),
        b"new image"
    );
    assert!(!fixture
        .path("destination.cgnb.assets")
        .join(old_digest)
        .exists());
}

#[test]
fn invalid_inputs_and_unknown_commands_fail() {
    let fixture = Fixture::new();
    let bad = fixture.write("invalid.cgnb", "not a notebook\n");
    let invalid = run(&[Path::new("inspect"), &bad]);
    assert!(!invalid.status.success());
    let invalid_bytes = fixture.path("invalid-utf8.cgnb");
    fs::write(&invalid_bytes, b"#!cgnb 1\n\xff").unwrap();
    let invalid = run(&[Path::new("inspect"), &invalid_bytes]);
    assert!(!invalid.status.success());
    assert!(stderr(&invalid).contains("UTF-8"));
    let wrong_ext = fixture.write("wrong.txt", INPUT);
    assert!(!run(&[Path::new("inspect"), &wrong_ext]).status.success());
    assert!(!run_str(&["surprise"]).status.success());
    let help = run_str(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("pure scalar"));
}
