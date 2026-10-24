#!/usr/bin/env python3
"""Additive ASan/LSan coverage for the new default managed ScopedRoots helpers.

Default leak mode requires a deliberately leaking negative control to be caught
before running the lifetime corpus. detect_leaks=0 is recorded as non-leak-only.
Exit 0 = passed, 1 = failed, 2 = blocked. Logs and JSON are always retained.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from datetime import datetime, timezone


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--detect-leaks", choices=("0", "1"), default="1")
    parser.add_argument("--toolchain", default="1.98.1")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    # Respect other caller diagnostics, but record and explicitly pin leak mode.
    inherited = os.environ.get("ASAN_OPTIONS", "")
    options = [s for s in inherited.split(":") if s and not s.startswith("detect_leaks=")]
    options.append(f"detect_leaks={args.detect_leaks}")
    env = dict(os.environ, RUSTUP_TOOLCHAIN=args.toolchain, RUSTC_BOOTSTRAP="1",
               ASAN_OPTIONS=":".join(options), TMPDIR=str(out))
    report = {
        "schema_version": 1, "status": "unrun", "scope": "Additive default managed ScopedRoots lifetime coverage",
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "leak_acceptance": args.detect_leaks == "1", "toolchain": args.toolchain,
        "asan_options": env["ASAN_OPTIONS"], "inherited_asan_options": inherited,
        "lsan_options": env.get("LSAN_OPTIONS", ""),
        "rustc_bootstrap": env["RUSTC_BOOTSTRAP"], "commands": [], "source_sha256": {},
    }

    def finish(status, reason):
        report.update(status=status, reason=reason)
        (out / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{status}: {reason}; evidence: {out / 'result.json'}")
        return {"passed": 0, "failed": 1, "blocked": 2}[status]

    def run(label, command):
        try:
            result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                    text=True, timeout=180)
        except (OSError, subprocess.TimeoutExpired) as error:
            report["commands"].append({"label": label, "argv": command, "error": str(error)})
            return None
        (out / f"{label}.stdout").write_text(result.stdout)
        (out / f"{label}.stderr").write_text(result.stderr)
        report["commands"].append({"label": label, "argv": command, "exit_code": result.returncode,
                                    "stdout": f"{label}.stdout", "stderr": f"{label}.stderr"})
        return result

    def blocked(result):
        if result is None:
            return True
        diagnostic = (result.stdout + result.stderr).lower()
        return any(s in diagnostic for s in (
            "does not work under ptrace", "leaksanitizer has encountered a fatal error",
            "sanitizer is not supported", "sanitizer runtime", "toolchain '"))

    if not shutil.which("rustc"):
        return finish("blocked", "rustc missing from PATH")
    identity = run("rustc-version", ["rustc", "-vV"])
    if identity is None or identity.returncode:
        return finish("blocked", "requested installed Rust toolchain unavailable")
    report["rustc_version"] = identity.stdout
    host = next((s.split(": ", 1)[1] for s in identity.stdout.splitlines()
                 if s.startswith("host: ")), None)
    if not host:
        return finish("blocked", "rustc host target unavailable")
    report["target"] = host
    flags = ["--edition=2021", "--target", host, "-Zsanitizer=address", "-Cdebuginfo=1",
             "-Copt-level=1", "--cfg", 'feature="fault-injection"']
    library = out / "libcgx2_runtime.rlib"
    sources = [root / "crates/cgx2-runtime/src/lib.rs",
               root / "crates/cgx2-memory-conformance/tests/default_managed_roots.rs"]
    provenance = list((root / "crates/cgx2-runtime/src").glob("*.rs")) + sources[1:] + [Path(__file__).resolve()]
    report["source_sha256"] = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in provenance}
    commands = [("runtime-build", ["rustc", *flags, "--crate-name", "cgx2_runtime", "--crate-type=lib",
                                   str(sources[0]), "-o", str(library)])]
    for label, command in commands:
        result = run(label, command)
        if result is None or result.returncode:
            return finish("blocked" if blocked(result) else "failed", f"{label} unavailable or failed")

    if args.detect_leaks == "1":
        probe = out / "leak_probe.rs"
        probe.write_text('''#[inline(never)]
fn leak() { let data = Box::new([42u8; 4096]);
    std::hint::black_box(data.as_ptr()); std::mem::forget(data); }
fn main() { leak(); }
''')
        probe_binary = out / "leak-probe"
        built = run("leak-probe-build", ["rustc", *flags, str(probe), "-o", str(probe_binary)])
        if built is None or built.returncode:
            return finish("blocked" if blocked(built) else "failed", "leak negative control build failed")
        detected = run("leak-probe-run", [str(probe_binary)])
        if blocked(detected):
            return finish("blocked", "host cannot run LeakSanitizer (see negative control diagnostics)")
        if not detected.returncode or "detected memory leaks" not in detected.stderr:
            return finish("blocked", "leak detection negative control was not detected")
        report["negative_control_detected"] = True

    for source in sources:
        label = "runtime-unit" if source == sources[0] else source.stem
        binary = out / label
        built = run(f"{label}-build", ["rustc", *flags, "--test", str(source), "--extern",
                                       f"cgx2_runtime={library}", "-o", str(binary)])
        if built is None or built.returncode:
            return finish("blocked" if blocked(built) else "failed", f"{label} build unavailable or failed")
        tested = run(f"{label}-run", [str(binary), "--test-threads=1"])
        if tested is None or tested.returncode:
            return finish("blocked" if blocked(tested) else "failed", f"{label} run unavailable or failed")
        if any(s in tested.stderr for s in ("ERROR: AddressSanitizer", "ERROR: LeakSanitizer")):
            return finish("failed", f"{label} sanitizer diagnostic despite zero exit")
    return finish("passed", "leak-enabled lifetime corpus" if args.detect_leaks == "1"
                  else "ASan only; detect_leaks=0 does not close leak acceptance")


if __name__ == "__main__":
    sys.exit(main())
