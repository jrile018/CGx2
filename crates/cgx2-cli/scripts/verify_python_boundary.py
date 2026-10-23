#!/usr/bin/env python3
"""Inspect exact built ELF artifacts and live native/worker process maps on Linux."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


def artifact(path):
    dynamic = subprocess.check_output(["readelf", "-d", path], text=True)
    ldd = subprocess.check_output(["ldd", path], text=True)
    assert "libpython" not in dynamic.lower(), dynamic
    assert "libpython" not in ldd.lower(), ldd
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "bytes": path.stat().st_size, "needed": re.findall(r"Shared library: \[(.*?)\]", dynamic), "ldd": ldd}


def observe(binary, source, env, expected, expect_worker=False, expect_torch=False, maps_only=False):
    binaries = {binary.resolve(), binary.with_name("cgx2-native").resolve()}
    process = subprocess.Popen([str(binary), "run", str(source)], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    native_paths = set()
    worker_paths = set()
    samples = 0
    stopped_after_torch_maps = False
    deadline = time.monotonic() + (180 if expect_torch else 15)
    try:
        while process.poll() is None:
            if time.monotonic() > deadline:
                raise AssertionError("verification deadline exceeded")
            try:
                executable = Path(f"/proc/{process.pid}/exe").resolve(strict=True)
                maps = Path(f"/proc/{process.pid}/maps").read_text()
                if executable in binaries:
                    assert "libpython" not in maps.lower(), maps
                    samples += 1
                    native_paths.update(line.split()[-1] for line in maps.splitlines() if "/" in line)
                    if expect_torch and maps_only and any("libtorch_cpu.so" in path for path in native_paths):
                        stopped_after_torch_maps = True
                        process.terminate()
                        break
                for child in Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().split():
                    child_exe = Path(f"/proc/{child}/exe").resolve(strict=True)
                    worker_paths.add(str(child_exe))
            except (FileNotFoundError, ProcessLookupError):
                pass
            time.sleep(0.002 if expect_torch else 0.0001)
        stdout, stderr = process.communicate(timeout=1)
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
    if not stopped_after_torch_maps:
        assert process.returncode == 0, stderr.decode(errors="replace")
        assert stdout.decode().strip() == expected, stdout
    assert samples > 0, "no maps sampled; rerun verification"
    python_children = {path for path in worker_paths if "python" in Path(path).name.lower() or "pypy" in Path(path).name.lower()}
    if expect_worker:
        selected = str(Path(env["CGX2_PYTHON"]).resolve())
        assert selected in worker_paths, "did not observe the selected explicit worker process"
        assert "Python worker executable=" in stderr.decode(), stderr
    else:
        assert not python_children, python_children
    if expect_torch:
        assert any("libtorch_cpu.so" in path for path in native_paths), "real LibTorch not observed"
    return {"source": str(source), "completed": not stopped_after_torch_maps, "map_samples": samples, "mapped_files": sorted(native_paths),
            "child_executables": sorted(worker_paths), "python_child_executables": sorted(python_children), "stdout": stdout.decode(), "stderr": stderr.decode()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", required=True, type=Path)
    parser.add_argument("--real-f32-package", type=Path)
    parser.add_argument("--real-maps-only", action="store_true", help="stop native real process after observing mapped LibTorch; makes no numeric claim")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    directory = args.binary_dir.resolve()
    binary = directory / "cgx2"
    report = {"artifacts": [artifact(binary), artifact(directory / "cgx2-native")]}
    env = dict(os.environ, CGX2_PYTHON="/missing/python", CGX2_PYTHON_WORKER="/missing/worker")
    with tempfile.TemporaryDirectory(prefix="cgx2-boundary-evidence-") as temporary:
        root = Path(temporary)
        scalar = root / "scalar.cg"
        scalar.write_text("let mut count = 0\nfor i in range(0, 5000):\n    count += 1\ncount\n")
        report["native_scalar"] = observe(binary, scalar, env, "5000")
        python = root / "explicit.cg"
        python.write_text('foreign python fn root(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime "local:cpython" ownership copied\n\nfn main() -> i64 effects(Foreign) caps(ForeignCall):\n    root(81)\n')
        python_env = dict(os.environ, CGX2_PYTHON="/usr/bin/python3", CGX2_PYTHON_TRACE="1")
        python_env.pop("CGX2_PYTHON_WORKER", None)
        report["explicit_python"] = observe(binary, python, python_env, "9", expect_worker=True)
        if args.real_f32_package:
            package = args.real_f32_package.resolve()
            real_env = dict(env, LD_LIBRARY_PATH=str(package))
            report["real_f32"] = observe(binary, package, real_env, "[[58.0, 64.0], [139.0, 154.0]]", expect_torch=True, maps_only=args.real_maps_only)
            report["real_package_pins"] = {name: hashlib.sha256((package/name).read_bytes()).hexdigest()
                                           for name in ("cgx2-bind.lock.json", "cgx2-libtorch-closure.lock.json", "cgx2-libtorch-evidence.json")}
        else:
            report["real_f32"] = {"skipped": "no reviewed --real-f32-package supplied"}
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
