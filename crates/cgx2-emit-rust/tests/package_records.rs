use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::{fs, path::Path, process::Command};

#[test]
fn public_record_package_debug_release_and_selected_failure() {
    let package =
        load_checked_package(Path::new("../cgx2-package/fixtures/public-records")).unwrap();
    let generated = emit_package_rust(&package).unwrap();
    assert_eq!(generated, emit_package_rust(&package).unwrap());
    let dir = std::env::temp_dir().join(format!("cgx2-public-record-rust-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.rs"), generated).unwrap();
    for release in [false, true] {
        let mut rustc = Command::new("rustc");
        rustc
            .arg("--edition=2024")
            .arg(dir.join("main.rs"))
            .arg("-o")
            .arg(dir.join("main"));
        if release {
            rustc.arg("-O");
        }
        let output = rustc.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(dir.join("main")).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            b"demo.geometry.Point(self: 42, live: true)\n"
        );
        let output = Command::new(dir.join("main")).arg("fail").output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CGR005"));
        let geometry =
            std::fs::read_to_string("../cgx2-package/fixtures/public-records/src/geometry.cg")
                .unwrap();
        let start = geometry.find("9223372036854775807 + 1").unwrap();
        let end = start + "9223372036854775807 + 1".len();
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("at {start}..{end}")));

        for (count, code) in [(2, "CGR006"), (3, "CGR005")] {
            let output = Command::new(dir.join("main"))
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
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn same_basename_unicode_and_generated_looking_names_do_not_collide() {
    let package = load_checked_package(Path::new(
        "../cgx2-package/fixtures/public-record-collisions",
    ))
    .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "cgx2-public-record-rust-collisions-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.rs"), emit_package_rust(&package).unwrap()).unwrap();
    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("-O")
        .arg(dir.join("main.rs"))
        .arg("-o")
        .arg(dir.join("main"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("main")).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"42\n");
    fs::remove_dir_all(dir).unwrap();
}
