use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_notebook_format::{
    open_notebook_bytes, parse_notebook_bytes, save_asset, Output as CellOutput,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn record_functions_run_inspect_and_clean_sweep_agree() {
    let fixture = Fixture::new();
    let example = include_str!("../examples/record-functions.cgnb");
    let input = fixture.write("records.cgnb", example);
    let output = fixture.path("executed.cgnb");
    let result = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    let executed = parse_notebook_bytes(&fs::read(&output).unwrap()).unwrap();
    let read = executed
        .cells
        .iter()
        .find(|cell| cell.id == "read")
        .unwrap();
    assert!(matches!(&read.outputs[0], CellOutput::Text { body, .. } if body == "42"));
    let result = run(&[Path::new("inspect"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(String::from_utf8_lossy(&result.stdout).contains("functions\tcode\tcached"));

    let sweep = fixture.path("sweep-records");
    let result = run_str(&[
        "sweep",
        input.to_str().unwrap(),
        "parameter",
        "--value",
        "2",
        "--value",
        "-1",
        "--out-dir",
        sweep.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    for (index, delta) in [2, -1].iter().enumerate() {
        let trial = parse_notebook_bytes(
            &fs::read(sweep.join(format!("trial-{:02}.cgnb", index + 1))).unwrap(),
        )
        .unwrap();
        let clean_input = fixture.write(
            "clean.cgnb",
            &example.replace("let delta = 1", &format!("let delta = {delta}")),
        );
        let clean_output = fixture.path(&format!("clean-result-{index}.cgnb"));
        let result = run(&[
            Path::new("run"),
            &clean_input,
            Path::new("--out"),
            &clean_output,
        ]);
        assert!(result.status.success(), "{}", stderr(&result));
        let clean = parse_notebook_bytes(&fs::read(clean_output).unwrap()).unwrap();
        for (trial, clean) in trial.cells.iter().zip(&clean.cells) {
            assert_eq!(trial.outputs, clean.outputs, "cell {}", trial.id);
        }
    }
}

#[test]
fn pure_function_fixture_runs_and_inspects_cached_status() {
    let fixture = Fixture::new();
    let input = fixture.write(
        "functions.cgnb",
        include_str!("../../cgx2-notebook-kernel/tests/fixtures/pure-functions.cgnb"),
    );
    let output = fixture.path("result.cgnb");
    let result = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    let saved = parse_notebook_bytes(&fs::read(&output).unwrap()).unwrap();
    assert!(matches!(&saved.cells[1].outputs[0], CellOutput::Text { body, .. } if body == "5"));
    let inspected = run(&[Path::new("inspect"), &output]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let listing = String::from_utf8(inspected.stdout).unwrap();
    assert!(listing.contains("defs\tcode\tcached"));
    assert!(listing.contains("value\tcode\tcached"));
    assert!(listing.contains("independent\tcode\tcached"));
}

#[test]
fn delete_function_cell_keeps_unrelated_cached_output() {
    let fixture = Fixture::new();
    let input = fixture.write(
        "functions.cgnb",
        include_str!("../../cgx2-notebook-kernel/tests/fixtures/pure-functions.cgnb"),
    );
    let executed = fixture.path("executed.cgnb");
    let deleted = fixture.path("deleted.cgnb");
    assert!(
        run(&[Path::new("run"), &input, Path::new("--out"), &executed])
            .status
            .success()
    );
    let result = run(&[
        Path::new("delete"),
        &executed,
        Path::new("defs"),
        Path::new("--out"),
        &deleted,
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let notebook = parse_notebook_bytes(&fs::read(&deleted).unwrap()).unwrap();
    assert_eq!(notebook.cells.len(), 2);
    assert_eq!(notebook.cells[0].id, "value");
    assert!(notebook.cells[0].provenance.as_ref().unwrap().stale);
    assert!(!notebook.cells[1].provenance.as_ref().unwrap().stale);
    let inspected = run(&[Path::new("inspect"), &deleted]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let listing = String::from_utf8(inspected.stdout).unwrap();
    assert!(listing.contains("value\tcode\tstale"));
    assert!(listing.contains("independent\tcode\tcached"));
}

#[test]
fn edited_function_reruns_only_its_cli_readers() {
    let fixture = Fixture::new();
    let input = fixture.write(
        "functions.cgnb",
        include_str!("../../cgx2-notebook-kernel/tests/fixtures/pure-functions.cgnb"),
    );
    let baseline = fixture.path("baseline.cgnb");
    let revised = fixture.path("revised.cgnb");
    assert!(
        run(&[Path::new("run"), &input, Path::new("--out"), &baseline])
            .status
            .success()
    );
    let source = "fn inc(x: i64) -> i64:\n    x + 2\n\nfn twice(x: i64) -> i64:\n    inc(inc(x))\n";
    let result = run(&[
        Path::new("run"),
        &input,
        Path::new("--out"),
        &revised,
        Path::new("--edit-cell"),
        Path::new("defs"),
        Path::new("--source"),
        Path::new(source),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let before = parse_notebook_bytes(&fs::read(&baseline).unwrap()).unwrap();
    let after = parse_notebook_bytes(&fs::read(&revised).unwrap()).unwrap();
    assert!(matches!(&after.cells[1].outputs[0], CellOutput::Text { body, .. } if body == "7"));
    assert_eq!(before.cells[2].provenance, after.cells[2].provenance);
    assert_ne!(
        before.cells[1].provenance.as_ref().unwrap().prefix_sha256,
        after.cells[1].provenance.as_ref().unwrap().prefix_sha256
    );
}

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

#[test]
fn project_option_runs_checked_two_module_example() {
    let fixture = Fixture::new();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pure-imports");
    let input = project.join("pure-imports.cgnb");
    let output = fixture.path("pure-imports-result.cgnb");
    let missing = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("--project"));
    let result = run(&[
        Path::new("run"),
        &input,
        Path::new("--project"),
        &project,
        Path::new("--out"),
        &output,
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let saved = parse_notebook_bytes(&fs::read(&output).unwrap()).unwrap();
    assert!(matches!(&saved.cells[3].outputs[0], CellOutput::Text { body, .. } if body == "43"));
    assert!(matches!(&saved.cells[4].outputs[0], CellOutput::Text { body, .. } if body == "21"));
    assert!(saved.cells[4]
        .provenance
        .as_ref()
        .unwrap()
        .package
        .as_ref()
        .unwrap()
        .modules
        .contains_key("demo.beta"));
}

const INPUT: &str = "#!cgnb 1\n%% markdown id=notes\n# Reference: figure.png\n%% code id=seed\nlet x = 3\n%% output id=seed mime=text/plain ordinal=9\nstale\n%% code id=result\nx + 4\n%% output id=result mime=text/plain ordinal=10\nstale\n";

#[test]
fn sweep_saves_independent_trials_and_reopens_failure_provenance() {
    let fixture = Fixture::new();
    let input = fixture.write("input.cgnb", include_str!("../examples/sweep-factor.cgnb"));
    let output = fixture.path("sweep");
    let result = run_str(&[
        "sweep",
        input.to_str().unwrap(),
        "factor",
        "--value",
        "2",
        "--value",
        "0",
        "--value",
        "5",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let summary = fs::read_to_string(output.join("summary.json")).unwrap();
    assert!(summary.contains("\"status\":\"failed\""));
    assert!(summary.contains("division by zero"));
    let first = output.join("trial-01.cgnb");
    let second = output.join("trial-02.cgnb");
    let third = output.join("trial-03.cgnb");
    let one = parse_notebook_bytes(&fs::read(&first).unwrap()).unwrap();
    let two = parse_notebook_bytes(&fs::read(&second).unwrap()).unwrap();
    let three = parse_notebook_bytes(&fs::read(&third).unwrap()).unwrap();
    assert_eq!(one.version, 1);
    assert!(matches!(&one.cells[2].outputs[0], CellOutput::Text { body, .. } if body == "5"));
    assert!(matches!(&three.cells[2].outputs[0], CellOutput::Text { body, .. } if body == "2"));
    assert!(
        two.cells[2].provenance.as_ref().unwrap().result
            == cgx2_notebook_format::ProvenanceResult::Failed
    );
    assert!(two.cells[3].outputs.is_empty());
    let inspect_one = run(&[Path::new("inspect"), &first]);
    let inspect_two = run(&[Path::new("inspect"), &second]);
    assert!(String::from_utf8_lossy(&inspect_one.stdout).contains("quotient\tcode\tcached"));
    assert!(String::from_utf8_lossy(&inspect_two.stdout).contains("quotient\tcode\tfailed"));
    let restarted = fixture.path("restarted.cgnb");
    assert!(
        run(&[Path::new("restart"), &first, Path::new("--out"), &restarted])
            .status
            .success()
    );
    let reopened = run(&[Path::new("inspect"), &restarted]);
    assert!(String::from_utf8_lossy(&reopened.stdout).contains("quotient\tcode\tstale"));
}

#[test]
fn sweep_embeds_two_markdown_images_in_success_and_failed_trials() {
    let fixture = Fixture::new();
    let input = fixture.path("input.cgnb");
    let assets = fixture.path("input.cgnb.assets");
    let first = save_asset(&assets, b"\x89PNG\r\n\x1a\nfirst").unwrap();
    let second = save_asset(&assets, b"GIF89asecond").unwrap();
    let source = include_str!("../examples/sweep-factor.cgnb").replace(
        "# Pure scalar sweep",
        &format!("![one](asset:sha256:{first})\n![two](asset:sha256:{second})"),
    );
    fs::write(&input, source).unwrap();
    let output = fixture.path("sweep-images");
    let args = [
        "sweep",
        input.to_str().unwrap(),
        "factor",
        "--value",
        "2",
        "--value",
        "0",
        "--out-dir",
        output.to_str().unwrap(),
    ];
    let result = run_str(&args);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        !run_str(&args).status.success(),
        "existing output must not be overwritten"
    );
    fs::remove_file(input).unwrap();
    fs::remove_dir_all(assets).unwrap();
    for (index, expected) in [
        (1, cgx2_notebook_format::ProvenanceResult::Success),
        (2, cgx2_notebook_format::ProvenanceResult::Failed),
    ] {
        let bytes = fs::read(output.join(format!("trial-{index:02}.cgnb"))).unwrap();
        let opened = open_notebook_bytes(&bytes, &fixture.path("gone.assets")).unwrap();
        assert_eq!(opened.assets.len(), 2);
        assert!(opened.assets.contains_key(&first));
        assert!(opened.assets.contains_key(&second));
        assert_eq!(
            opened.notebook.cells[2].provenance.as_ref().unwrap().result,
            expected
        );
        assert!(String::from_utf8_lossy(&bytes).contains("%% embedded sha256="));
    }
    let rerun = fixture.path("rerun.cgnb");
    let trial = output.join("trial-01.cgnb");
    let result = run(&[Path::new("run"), &trial, Path::new("--out"), &rerun]);
    assert!(result.status.success(), "{}", stderr(&result));
    let reopened =
        open_notebook_bytes(&fs::read(rerun).unwrap(), &fixture.path("gone.assets")).unwrap();
    assert_eq!(reopened.assets.len(), 2);
}

#[test]
fn sweep_rejects_invalid_values_effects_and_output_paths_before_publication() {
    let fixture = Fixture::new();
    let input = fixture.write("input.cgnb", include_str!("../examples/sweep-factor.cgnb"));
    let oversized = "x".repeat(4097);
    for (name, values) in [
        ("type", vec!["2", "true"]),
        ("duplicate", vec!["2", "2"]),
        ("oversized", vec!["2", oversized.as_str()]),
    ] {
        let output = fixture.path(name);
        let mut args = vec!["sweep", input.to_str().unwrap(), "factor"];
        for value in values {
            args.extend(["--value", value]);
        }
        args.extend(["--out-dir", output.to_str().unwrap()]);
        assert!(!run_str(&args).status.success(), "{name}");
        assert!(!output.exists(), "{name} published files");
    }
    let effect = fixture.write(
        "effect.cgnb",
        "#!cgnb 1\n%% code id=factor\nlet factor = 2\n%% code id=effect\nprint(\"side effect\")\n",
    );
    let output = fixture.path("effect-output");
    let result = run_str(&[
        "sweep",
        effect.to_str().unwrap(),
        "factor",
        "--value",
        "3",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!output.exists());
    let bad_id = fixture.path("bad-id");
    assert!(!run_str(&[
        "sweep",
        input.to_str().unwrap(),
        "missing",
        "--value",
        "3",
        "--out-dir",
        bad_id.to_str().unwrap(),
    ])
    .status
    .success());
    assert!(!bad_id.exists());
    let traversal = fixture.path("../outside");
    assert!(!run_str(&[
        "sweep",
        input.to_str().unwrap(),
        "factor",
        "--value",
        "3",
        "--out-dir",
        traversal.to_str().unwrap()
    ])
    .status
    .success());
    assert!(!fixture.path("outside").exists());
}

#[test]
fn sweep_accepts_bool_and_string_literals_and_rejects_collisions() {
    let fixture = Fixture::new();
    for (name, original, values) in [
        ("bool", "true", ["true", "false"]),
        ("string", "\"a\"", ["\"a\"", "\"b\""]),
    ] {
        let input = fixture.write(&format!("{name}.cgnb"), &format!("#!cgnb 1\n%% code id=parameter\nlet parameter = {original}\n%% code id=result\nparameter\n"));
        let output = fixture.path(&format!("{name}-sweep"));
        let args = [
            "sweep",
            input.to_str().unwrap(),
            "parameter",
            "--value",
            values[0],
            "--value",
            values[1],
            "--out-dir",
            output.to_str().unwrap(),
        ];
        let result = run_str(&args);
        assert!(result.status.success(), "{}", stderr(&result));
        let first = parse_notebook_bytes(&fs::read(output.join("trial-01.cgnb")).unwrap()).unwrap();
        let second =
            parse_notebook_bytes(&fs::read(output.join("trial-02.cgnb")).unwrap()).unwrap();
        assert_ne!(first.cells[1].outputs, second.cells[1].outputs);
        assert!(
            !run_str(&args).status.success(),
            "existing destination must be rejected"
        );
    }
}

#[cfg(unix)]
#[test]
fn sweep_rejects_symlink_input_and_destination() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let input = fixture.write("input.cgnb", include_str!("../examples/sweep-factor.cgnb"));
    let linked_input = fixture.path("linked.cgnb");
    symlink(&input, &linked_input).unwrap();
    let output = fixture.path("sweep");
    let result = run_str(&[
        "sweep",
        linked_input.to_str().unwrap(),
        "factor",
        "--value",
        "2",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!output.exists());
    let other = fixture.path("other");
    fs::create_dir(&other).unwrap();
    symlink(&other, &output).unwrap();
    let result = run_str(&[
        "sweep",
        input.to_str().unwrap(),
        "factor",
        "--value",
        "2",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
}

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
        [CellOutput::Text { mime, ordinal: 11, body }]
            if mime == "text/plain" && body.trim() == "7"
    ));
    assert!(notebook.cells[2].provenance.is_some());
    let inspected = run(&[Path::new("inspect"), &output]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let text = String::from_utf8_lossy(&inspected.stdout);
    assert!(
        text.contains("result\tcode\tcached\tschema=2\tordinal="),
        "{text}"
    );
    assert!(text.contains("source_sha256="), "{text}");
}

#[test]
fn restart_roundtrip_preserves_saved_revision_as_stale_without_running() {
    let fixture = Fixture::new();
    let input = fixture.write("source.cgnb", INPUT);
    let executed = fixture.path("executed.cgnb");
    assert!(
        run(&[Path::new("run"), &input, Path::new("--out"), &executed])
            .status
            .success()
    );
    let stale = fixture.path("stale.cgnb");
    let result = run(&[Path::new("restart"), &executed, Path::new("--out"), &stale]);
    assert!(result.status.success(), "{}", stderr(&result));
    let inspected = run(&[Path::new("inspect"), &stale]);
    let listing = String::from_utf8(inspected.stdout).unwrap();
    assert!(listing.contains("result\tcode\tstale"), "{listing}");
    let saved = parse_notebook_bytes(&fs::read(&stale).unwrap()).unwrap();
    assert_eq!(
        saved.cells[2].outputs,
        parse_notebook_bytes(&fs::read(&executed).unwrap())
            .unwrap()
            .cells[2]
            .outputs
    );
    assert_eq!(
        saved.cells[2].provenance.as_ref().unwrap().result,
        cgx2_notebook_format::ProvenanceResult::Success
    );
    assert!(saved.cells[2].provenance.as_ref().unwrap().stale);
    assert!(!run(&[
        Path::new("run"),
        &stale,
        Path::new("--out"),
        &fixture.path("bad.cgnb"),
        Path::new("--timeout-ms"),
        Path::new("0")
    ])
    .status
    .success());
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
    assert!(fs::read_to_string(input).unwrap().contains("ordinal=11\n7"));
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
        "#!cgnb 1\n%% code id=unsupported\nfn main() -> i64:\n    2\n",
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
    assert!(stderr(&failed).contains("CGNK011"));
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
#[test]
fn edited_run_saves_output_and_preserves_independent_execution_revision() {
    let fixture = Fixture::new();
    let input = fixture.write("input.cgnb",
        "#!cgnb 1\n%% code id=seed\nlet x = 3\n%% code id=independent\nlet z = 9\n%% code id=result\nx + 4\n");
    let output = fixture.path("output.cgnb");
    let result = run_str(&[
        "run",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
        "--edit-cell",
        "seed",
        "--source",
        "let x = 5",
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let notebook = parse_notebook_bytes(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(notebook.cells[0].provenance.as_ref().unwrap().ordinal, 4);
    assert_eq!(notebook.cells[1].provenance.as_ref().unwrap().ordinal, 2);
    assert_eq!(notebook.cells[2].provenance.as_ref().unwrap().ordinal, 5);
    assert!(matches!(&notebook.cells[2].outputs[0], CellOutput::Text { body, .. } if body == "9"));
    let shown = run_str(&["show", output.to_str().unwrap(), "result"]);
    assert!(shown.status.success(), "{}", stderr(&shown));
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("schema=2 execution_revision=5"));
    assert!(text.contains("mime=text/plain"));
    assert!(text.contains("text=\"9\""));
}

#[test]
fn two_cell_fixture_runs_edits_saves_exports_and_reopens_without_execution() {
    let fixture = Fixture::new();
    let input = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/two-cells.cgnb"
    ));
    let saved = fixture.path("saved.cgnb");
    let portable = fixture.path("portable.cgnb");
    let run_result = run_str(&[
        "run",
        input.to_str().unwrap(),
        "--out",
        saved.to_str().unwrap(),
        "--edit-cell",
        "seed",
        "--source",
        "let x = 5",
    ]);
    assert!(run_result.status.success(), "{}", stderr(&run_result));
    let shown = run_str(&["show", saved.to_str().unwrap(), "result"]);
    assert!(shown.status.success(), "{}", stderr(&shown));
    let display = String::from_utf8(shown.stdout).unwrap();
    assert!(display.contains("schema=2 execution_revision=4"));
    assert!(display.contains("text=\"9\""));
    let exported = run_str(&[
        "export",
        saved.to_str().unwrap(),
        "--out",
        portable.to_str().unwrap(),
    ]);
    assert!(exported.status.success(), "{}", stderr(&exported));
    let reopened = run_str(&["show", portable.to_str().unwrap(), "result"]);
    assert!(reopened.status.success(), "{}", stderr(&reopened));
    assert_eq!(reopened.stdout, display.as_bytes());
}

#[test]
fn pure_record_fixture_runs_and_reopens_with_cached_provenance() {
    let fixture = Fixture::new();
    let input = fixture.write(
        "records.cgnb",
        include_str!("../../cgx2-notebook-kernel/tests/fixtures/pure-records.cgnb"),
    );
    let output = fixture.path("records-out.cgnb");
    let result = run(&[Path::new("run"), &input, Path::new("--out"), &output]);
    assert!(result.status.success(), "{}", stderr(&result));
    let saved = parse_notebook_bytes(&fs::read(&output).unwrap()).unwrap();
    assert!(
        matches!(&saved.cells[3].outputs[0], CellOutput::Text { body, .. } if body == "-9223372036854775808")
    );
    assert!(matches!(&saved.cells[4].outputs[0], CellOutput::Text { body, .. } if body == "true"));
    let inspected = run(&[Path::new("inspect"), &output]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    let listing = String::from_utf8(inspected.stdout).unwrap();
    assert!(listing.contains("point\tcode\tcached"));
    assert!(listing.contains("read\tcode\tcached"));
}
