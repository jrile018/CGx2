#!/usr/bin/env python3
"""Instrument the runtime and generated conditional-memory binaries.

Requires the already built cgx2c binary in CARGO_TARGET_DIR/debug and Rust
1.98.1's address sanitizer runtime. Leaves reproducible artifacts under the
private target directory. Leak detection is deliberately a separate gate.
"""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
TARGET = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
OUT = TARGET / "conditional-asan"
OUT.mkdir(exist_ok=True)
ENV = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", RUSTC_BOOTSTRAP="1")
RUST_FLAGS = ["--target", "x86_64-unknown-linux-gnu", "-Zsanitizer=address", "-Cdebuginfo=1"]
CASES = [
    ("conditional_memory_success.cg", None, 0),
    ("conditional_memory_functions.cg", None, 0),
    ("conditional_memory_graph.cg", None, 0),
    ("conditional_memory_auto.cg", None, 0),
    ("conditional_memory_auto.cg", "HostAllocation", 4),
    ("conditional_memory_region_failure.cg", "RegionLimit", 0),
    ("conditional_memory_budget_failure.cg", "BudgetExceeded", 0),
    ("conditional_memory_negative.cg", "negative", 0),
    ("conditional_memory_graph_failure.cg", "BudgetExceeded", 0),
    ("conditional_memory_region_failure.cg", "HostAllocation", 4),
    ("conditional_memory_region_failure.cg", "HostAllocation", 5),
]


def run(command, env=ENV):
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, check=False)
    if result.returncode:
        raise AssertionError(f"{command}\n{result.stdout}\n{result.stderr}")
    return result


for profile, level in [("debug", "0"), ("release", "3")]:
    folder = OUT / profile
    folder.mkdir(exist_ok=True)
    runtime = folder / "libcgx2_runtime.rlib"
    run(["rustc", "--edition=2021", "--crate-name", "cgx2_runtime", "--crate-type=lib",
         '--cfg', 'feature="fault-injection"', *RUST_FLAGS, f"-Copt-level={level}",
         str(ROOT / "crates/cgx2-runtime/src/lib.rs"), "-o", str(runtime)])
    for fixture, error, fault in CASES:
        label = f"{Path(fixture).stem}-{fault}"
        source = folder / f"{label}.rs"
        binary = folder / label
        report = folder / f"{label}.json"
        run([str(TARGET / "debug/cgx2c"), "emit-rust", str(ROOT / "crates/cgx2c/tests/fixtures" / fixture),
             "--out", str(source), "--force"])
        run(["rustc", "--edition=2024", *(['--cfg', 'feature="fault-injection"'] if fault else []),
             *RUST_FLAGS, f"-Copt-level={level}", str(source), "--extern",
             f"cgx2_runtime={runtime}", "-o", str(binary)], dict(ENV, CGX2_TEST_FAIL_RESERVE=str(fault)))
        executed = subprocess.run([str(binary)], cwd=ROOT,
                                  env=dict(ENV, ASAN_OPTIONS="detect_leaks=0", CGX2_MEMORY_REPORT=str(report)),
                                  capture_output=True, text=True, check=False)
        assert executed.returncode == (1 if error else 0), executed.stderr
        assert not executed.stdout
        assert "AddressSanitizer" not in executed.stderr, executed.stderr
        if error:
            assert error in executed.stderr, executed.stderr
        else:
            assert not executed.stderr, executed.stderr
        data = json.loads(report.read_text())
        assert data["status"] == ("error" if error else "ok")
        if error:
            assert data["cleanup_errors"] == []
            assert data["final_unwind"]["runtime_root_registrations"] == 0
        print(f"ASan {profile} {label}: passed", flush=True)
print(f"{2 * len(CASES)} generated binary runs passed; runtime and generated code instrumented; leaks disabled.")
