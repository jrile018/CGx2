use cgx2_emit_python::emit_package_python;
use cgx2_package::load_checked_package;
use std::{fs, path::Path, process::Command};

#[test]
fn public_record_package_is_standalone_and_checks_nominal_inputs_results() {
    let package =
        load_checked_package(Path::new("../cgx2-package/fixtures/public-records")).unwrap();
    let files = emit_package_python(&package).unwrap();
    assert_eq!(files, emit_package_python(&package).unwrap());
    let dir =
        std::env::temp_dir().join(format!("cgx2-public-record-python-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in &files {
        fs::write(dir.join(file), text).unwrap();
    }
    let output = Command::new("python3").arg(&dir).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"demo.geometry.Point(self: 42, live: true)\n"
    );
    let output = Command::new("python3")
        .arg(&dir)
        .arg("fail")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CGR005"));
    for (count, code) in [(2, "CGR006"), (3, "CGR005")] {
        let output = Command::new("python3")
            .arg(&dir)
            .args(vec!["x"; count])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let module = files
        .keys()
        .find(|path| {
            path.to_string_lossy().starts_with("cgx2_mod_")
                && files[*path].contains("def identity(")
        })
        .unwrap()
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    let geometry =
        std::fs::read_to_string("../cgx2-package/fixtures/public-records/src/geometry.cg").unwrap();
    let failure_start = geometry.find("9223372036854775807 + 1").unwrap();
    let failure_end = failure_start + "9223372036854775807 + 1".len();
    let probe = format!("import {module} as m\ntry:\n    m.choose(False)\nexcept OverflowError as e:\n    assert e.code == 'CGR005' and e.span == ({failure_start}, {failure_end})\nelse:\n    raise AssertionError('missing checked failure')\nclass Other:\n    self = 42\n    live = True\ntry:\n    m.identity(Other())\nexcept TypeError as e:\n    assert 'CGR004' in str(e)\nelse:\n    raise AssertionError('accepted wrong nominal input')\nm.choose.__globals__['{}'] = lambda **kwargs: Other()\ntry:\n    m.choose(True)\nexcept TypeError as e:\n    assert 'wrong nominal record result' in str(e)\nelse:\n    raise AssertionError('accepted wrong nominal result')\n", "_cgx2_user_64656d6f2e67656f6d657472792e506f696e74");
    let output = Command::new("python3")
        .current_dir(&dir)
        .arg("-c")
        .arg(probe)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn same_basename_unicode_and_generated_looking_names_do_not_collide() {
    let package = load_checked_package(Path::new(
        "../cgx2-package/fixtures/public-record-collisions",
    ))
    .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "cgx2-public-record-python-collisions-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in emit_package_python(&package).unwrap() {
        fs::write(dir.join(file), text).unwrap();
    }
    let output = Command::new("python3").arg(&dir).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"42\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_pure_import_diamond_emits_and_preserves_runtime_errors_and_snapshot() {
    let root = std::env::temp_dir().join(format!(
        "cgx2-checked-effects-python-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let fixture = Path::new("../cgx2-package/fixtures/checked-effects");
    fs::copy(fixture.join("cgx2.toml"), root.join("cgx2.toml")).unwrap();
    for name in ["main", "join", "left", "right", "base"] {
        fs::copy(
            fixture.join(format!("src/{name}.cg")),
            root.join(format!("src/{name}.cg")),
        )
        .unwrap();
    }
    let base = root.join("src/base.cg");
    let pure = fs::read_to_string(&base).unwrap();
    for (source, failure) in [(&pure, false), (&pure.replace("x + 1", "x / 0"), true)] {
        fs::write(&base, source).unwrap();
        let package = load_checked_package(&root).unwrap();
        let files = emit_package_python(&package).unwrap();
        assert_eq!(files, emit_package_python(&package).unwrap());
        let output = root.join(format!("python-{failure}"));
        cgx2_emit_python::write_package_python(&package, &output).unwrap();
        let result = Command::new("python3").arg(&output).output().unwrap();
        assert_eq!(result.status.success(), !failure);
        if failure {
            assert!(String::from_utf8_lossy(&result.stderr).contains("CGR006"));
        } else {
            assert_eq!(result.stdout, b"42\n");
        }
    }
    let package = load_checked_package(&root).unwrap();
    let output = root.join("python-true");
    let previous = fs::read(output.join("__main__.py")).unwrap();
    fs::write(&base, pure.replace("x + 1", "arg_count()")).unwrap();
    assert!(load_checked_package(&root).is_err());
    assert_eq!(
        cgx2_emit_python::write_package_python(&package, &output).unwrap_err()[0].code,
        "CGP018"
    );
    assert_eq!(fs::read(output.join("__main__.py")).unwrap(), previous);
    fs::remove_dir_all(root).unwrap();
}
