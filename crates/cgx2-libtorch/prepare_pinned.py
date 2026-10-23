#!/usr/bin/env python3
"""Build a local schema-2 CPU facade package from an exact, already built LibTorch.

This never downloads code or libraries. Search directories are explicit; a
DT_NEEDED name with two distinct candidates is an error, not a search-order
choice. The evidence file supplements (and does not weaken) schema-2 preflight.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

MAX_LIBRARY = 2 * 1024**3
MAX_TOTAL = 8 * 1024**3
MAX_LIBRARIES = 128
MAX_READELF = 1024 * 1024
IDENTITY = "cgx2.libtorch.cpu"
ABI = "cgx2-libtorch-facade-v1"


def fail(message):
    raise ValueError(message)


def canonical_file(path):
    path = Path(path)
    resolved = path.resolve(strict=True)
    if not resolved.is_file() or path.is_symlink():
        fail(f"not a regular nonsymlink file: {path}")
    if path.absolute() != resolved:
        fail(f"noncanonical file path: {path}")
    return resolved


def canonical_dir(path):
    path = Path(path)
    resolved = path.resolve(strict=True)
    if not resolved.is_dir() or path.absolute() != resolved:
        fail(f"noncanonical directory path: {path}")
    return resolved


def digest(path):
    before = path.stat()
    if before.st_size > MAX_LIBRARY:
        fail(f"library exceeds 2 GiB: {path}")
    sha = hashlib.sha256()
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        opened = os.fstat(stream.fileno())
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            fail(f"library changed before hashing: {path}")
        while block := stream.read(1024 * 1024):
            sha.update(block)
        held = os.fstat(stream.fileno())
    after = path.stat()
    identity = lambda stat: (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns)
    if identity(before) != identity(held) or identity(before) != identity(after):
        fail(f"library changed during hashing: {path}")
    return sha.hexdigest(), after.st_size


def copy_pinned(source, target, expected_sha, expected_size):
    """Copy a verified ELF without following a destination symlink."""
    if target.is_symlink() or (target.exists() and not target.is_file()):
        fail(f"unsafe package library target: {target}")
    source_stat = source.stat()
    if source != target:
        if (target.exists() and not os.path.samefile(source, target)
                and digest(target) == (expected_sha, expected_size)):
            pass
        else:
            descriptor, temporary = tempfile.mkstemp(prefix=".cgx2-copy-", dir=target.parent)
            try:
                with os.fdopen(descriptor, "wb") as output:
                    input_descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
                    with os.fdopen(input_descriptor, "rb") as input_file:
                        opened = os.fstat(input_file.fileno())
                        if (opened.st_dev, opened.st_ino) != (source_stat.st_dev, source_stat.st_ino):
                            fail(f"source changed before package copy: {source}")
                        shutil.copyfileobj(input_file, output, 1024 * 1024)
                if digest(Path(temporary)) != (expected_sha, expected_size):
                    fail(f"source changed during package copy: {source}")
                os.replace(temporary, target)
            finally:
                if os.path.exists(temporary):
                    os.unlink(temporary)
    after = source.stat()
    if (source_stat.st_dev, source_stat.st_ino) != (after.st_dev, after.st_ino) or digest(source) != (expected_sha, expected_size):
        fail(f"source changed during package copy: {source}")
    if digest(canonical_file(target)) != (expected_sha, expected_size):
        fail(f"copied library differs from pinned source: {target}")


def elf_dynamic(path):
    result = subprocess.run(
        ["readelf", "-d", str(path)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=20,
        check=False,
    )
    if result.returncode or len(result.stdout) > MAX_READELF:
        fail(f"readelf failed or exceeded 1 MiB: {path}")
    output = result.stdout.decode("utf-8")
    if "Dynamic section" not in output:
        fail(f"no ELF dynamic section: {path}")
    tags = {"NEEDED": [], "SONAME": [], "RUNPATH": [], "RPATH": []}
    for line in output.splitlines():
        match = re.search(r"\((NEEDED|SONAME|RUNPATH|RPATH)\).*\[([^\]]*)\]", line)
        if match:
            tags[match.group(1)].append(match.group(2))
    if tags["RPATH"] or len(tags["SONAME"]) > 1 or len(tags["RUNPATH"]) > 1:
        fail(f"ambiguous or legacy ELF search metadata: {path}")
    runpath = tags["RUNPATH"][0] if tags["RUNPATH"] else ""
    for entry in filter(None, runpath.split(":")):
        prefix = "${ORIGIN}" if entry.startswith("${ORIGIN}") else "$ORIGIN"
        if not entry.startswith(prefix):
            fail(f"RUNPATH outside $ORIGIN: {path}: {entry}")
        suffix = entry[len(prefix):]
        if suffix and (not suffix.startswith("/") or any(
            part in ("", ".", "..") for part in suffix[1:].split("/")
        )):
            fail(f"unsafe RUNPATH: {path}: {entry}")
    return tags["SONAME"][0] if tags["SONAME"] else None, tags["NEEDED"], runpath


def symbol(name, parameters, result, ownership="none", release=None, error="none"):
    return {
        "name": name, "parameters": parameters, "result": result,
        "ownership": ownership, "owner": None, "release": release,
        "error": error, "effects": ["foreign"], "capabilities": ["native"],
    }


def facade_symbols(matrix=False, matrix_f32=False):
    symbols = [
        symbol("cgx2_library_identity", [], "cstr_ptr"),
        symbol("cgx2_library_version", [], "cstr_ptr"),
        symbol("cgx2_abi_version", [], "u32"),
        symbol("cgx2_torch_last_error", [], "cstr_ptr"),
        symbol("cgx2_torch_f64_cpu", ["f64_const_ptr", "u64", "handle_out_ptr"],
               "i32", "owned", "cgx2_torch_release", "status"),
        symbol("cgx2_torch_add",
               ["handle_const_ptr", "handle_const_ptr", "handle_out_ptr"],
               "i32", "owned", "cgx2_torch_release", "status"),
        symbol("cgx2_torch_length", ["handle_const_ptr", "u64_out_ptr"], "i32",
               error="status"),
        symbol("cgx2_torch_copy_f64", ["handle_const_ptr", "f64_mut_ptr", "u64"],
               "i32", error="status"),
        symbol("cgx2_torch_release", ["handle_mut_ptr"], "i32", error="status"),
    ]
    if matrix:
        symbols.extend([
            symbol("cgx2_torch_matrix_api_version", [], "u32"),
            symbol("cgx2_torch_matrix_f64_cpu",
                   ["f64_const_ptr", "u64", "u64", "handle_out_ptr"],
                   "i32", "owned", "cgx2_torch_release", "status"),
            symbol("cgx2_torch_matmul",
                   ["handle_const_ptr", "handle_const_ptr", "handle_out_ptr"],
                   "i32", "owned", "cgx2_torch_release", "status"),
            symbol("cgx2_torch_matrix_shape",
                   ["handle_const_ptr", "u64_out_ptr", "u64_out_ptr"],
                   "i32", error="status"),
            symbol("cgx2_torch_matrix_copy_f64",
                   ["handle_const_ptr", "f64_mut_ptr", "u64"],
                   "i32", error="status"),
        ])
    if matrix_f32:
        symbols.extend([
            symbol("cgx2_torch_matrix_f32_api_version", [], "u32"),
            symbol("cgx2_torch_matrix_f32_cpu",
                   ["f32_const_ptr", "u64", "u64", "handle_out_ptr"],
                   "i32", "owned", "cgx2_torch_release", "status"),
            symbol("cgx2_torch_matmul_f32",
                   ["handle_const_ptr", "handle_const_ptr", "handle_out_ptr"],
                   "i32", "owned", "cgx2_torch_release", "status"),
            symbol("cgx2_torch_matrix_f32_info",
                   ["handle_const_ptr", "u64_out_ptr", "u64_out_ptr", "u32_out_ptr"],
                   "i32", error="status"),
            symbol("cgx2_torch_matrix_copy_f32",
                   ["handle_const_ptr", "f32_mut_ptr", "u64"],
                   "i32", error="status"),
        ])
    return sorted(symbols, key=lambda item: item["name"])


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def compiler_default_abi(compiler):
    result = subprocess.run(
        [str(compiler), "-dM", "-E", "-x", "c++", "-"],
        input=b"#include <bits/c++config.h>\n",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=20,
        check=False,
    )
    if result.returncode or len(result.stdout) > MAX_READELF:
        fail("cannot inspect compiler default C++11 ABI")
    match = re.search(rb"^#define _GLIBCXX_USE_CXX11_ABI ([01])$", result.stdout, re.MULTILINE)
    if not match:
        fail("compiler does not declare default C++11 ABI")
    return match.group(1).decode()


def prepare(args):
    if platform.system() != "Linux":
        fail("schema-2 ELF preparation requires Linux")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        fail("version must be exact major.minor.patch")
    if args.cxx11_abi not in ("0", "1"):
        fail("cxx11 ABI must be 0 or 1")
    facade = canonical_file(args.facade)
    root = canonical_dir(args.libtorch_root)
    build_dir = canonical_dir(args.build_dir)
    cache = (build_dir / "CMakeCache.txt").read_text(encoding="utf-8")
    configured_flags = (build_dir / "cgx2-torch-cxx-flags.txt").read_text(encoding="utf-8")
    if configured_flags != args.torch_cxx_flags:
        fail("TORCH_CXX_FLAGS differs from configured facade build")
    cache_values = dict(re.findall(
        r"^([^\r\n/#][^\r\n:=]*):[^=\r\n]*=(.*)$", cache, re.MULTILINE
    ))
    if cache_values.get("CGX2_EXPECT_TORCH_VERSION") != args.version:
        fail("CMakeCache version does not match requested version")
    compiler = canonical_file(args.compiler)
    if Path(cache_values.get("CMAKE_CXX_COMPILER", "")).resolve() != compiler:
        fail("CMakeCache compiler does not match requested compiler")
    cmake_flags = [
        cache_values.get("CMAKE_CXX_FLAGS", ""),
        cache_values.get("CMAKE_CXX_FLAGS_RELEASE", ""),
    ]
    explicit_abi = re.findall(
        r"-D_GLIBCXX_USE_CXX11_ABI=([01])(?:\s|$)",
        " ".join([args.torch_cxx_flags, *cmake_flags]) + " ",
    )
    if len(set(explicit_abi)) > 1:
        fail("conflicting C++11 ABI definitions in CMake flags")
    effective_abi = explicit_abi[0] if explicit_abi else compiler_default_abi(compiler)
    if effective_abi != args.cxx11_abi:
        fail("configured flags or compiler default C++11 ABI differs from requested ABI")
    prefix = cache_values.get("CMAKE_PREFIX_PATH", "")
    if str(root) not in prefix.split(";"):
        fail("CMakeCache does not name exact LibTorch root in CMAKE_PREFIX_PATH")
    if facade.parent != build_dir:
        fail("facade must be the exact build-dir output")
    directories = [canonical_dir(root / "lib")]
    directories.extend(canonical_dir(item) for item in args.lib_dir)
    directories = list(dict.fromkeys(directories))
    selections = {}
    for selection in args.select_dependency:
        name, separator, path = selection.partition("=")
        if not separator or not name or name in selections:
            fail(f"invalid or duplicate dependency selection: {selection}")
        selections[name] = canonical_file(path)

    # Resolve the complete graph before writing any lock. Canonical paths keep
    # symlink aliases from becoming lock entries; distinct targets are ambiguous.
    discovered = {}
    pending = [(facade.name, facade, "facade")]
    total = 0
    while pending:
        name, path, role = pending.pop(0)
        if name in discovered:
            if discovered[name]["path"] != str(path):
                fail(f"ambiguous DT_NEEDED identity: {name}")
            continue
        if len(discovered) >= MAX_LIBRARIES:
            fail("closure exceeds 128 libraries")
        sha, size = digest(path)
        if role == "facade" and size > 256 * 1024**2:
            fail("facade exceeds binding preflight's 256 MiB limit")
        total += size
        if total > MAX_TOTAL:
            fail("closure exceeds 8 GiB")
        soname, needed, runpath = elf_dynamic(path)
        if name != (soname or path.name):
            fail(f"SONAME does not match dependency name: {name}")
        discovered[name] = {
            "name": name, "path": str(path), "role": role, "sha256": sha,
            "size": size, "soname": soname, "needed": sorted(needed),
            "runpath": runpath,
        }
        for dependency in needed:
            if "/" in dependency or dependency in ("", ".", ".."):
                fail(f"invalid DT_NEEDED name: {dependency}")
            candidates = set()
            for directory in directories:
                candidate = directory / dependency
                if candidate.exists():
                    resolved = candidate.resolve(strict=True)
                    if not resolved.is_file():
                        fail(f"nonfile DT_NEEDED candidate: {candidate}")
                    candidates.add(resolved)
            if dependency in selections:
                if selections[dependency] not in candidates:
                    fail(f"selected DT_NEEDED path is not a candidate: {dependency}")
                candidates = {selections[dependency]}
            if len(candidates) != 1:
                fail(f"missing or ambiguous DT_NEEDED {dependency}: {sorted(map(str, candidates))}")
            pending.append((dependency, candidates.pop(), "dependency"))

    package = Path(args.package).absolute()
    package.mkdir(parents=True, exist_ok=True)
    package = canonical_dir(package)
    torch_lib = canonical_dir(root / "lib")
    system_roots = (Path("/usr/lib"), Path("/lib"), Path("/lib64"))
    for entry in discovered.values():
        if (not entry["name"] or entry["name"] in (".", "..")
                or "/" in entry["name"] or "\\" in entry["name"]):
            fail(f"unsafe ELF library name: {entry['name']}")
        source = canonical_file(entry["path"])
        entry["source_path"] = str(source)
        # The ELF interpreter and glibc libraries are normally mapped before
        # dlopen. Pin their actual host paths instead of copying a library the
        # loader cannot select. Copy the facade and every non-system library.
        packaged = (source == facade or source.is_relative_to(torch_lib)
                    or not any(source.is_relative_to(directory) for directory in system_roots))
        entry["packaged"] = packaged
        if packaged:
            target = package / entry["name"]
            copy_pinned(source, target, entry["sha256"], entry["size"])
            entry["path"] = str(target)
    facade_entry = discovered[facade.name]
    closure = []
    for entry in sorted(discovered.values(), key=lambda item: item["name"]):
        closure.append({
            "role": entry["role"], "name": entry["name"], "path": entry["path"],
            "sha256": entry["sha256"], "os": "linux",
            "arch": platform.machine(), "soname": entry["soname"],
        })
    binding = {
        "id": "torch_cpu", "route": "libtorch", "library": IDENTITY,
        "version": args.version, "artifact": facade.name,
        "sha256": facade_entry["sha256"], "os": "linux",
        "arch": platform.machine(), "abi": ABI, "calling_convention": "c",
        "layout": "tensor-f32-cpu-matrix-v3" if args.matrix_f32 else
                  "tensor-f64-cpu-matrix-v2" if args.matrix else "tensor-f64-cpu-v1",
        "symbols": facade_symbols(args.matrix or args.matrix_f32, args.matrix_f32),
    }
    (package / "src").mkdir(exist_ok=True)
    canonical_dir(package / "src")
    for name in ("src/main.cg", "cgx2.toml", "cgx2-bind.lock.json",
                 "cgx2-libtorch-closure.lock.json", "cgx2-libtorch-evidence.json"):
        if (package / name).is_symlink():
            fail(f"symlink-substituted package file: {package / name}")
    (package / "src/main.cg").write_text("module libtorch_pinned\n", encoding="utf-8")
    (package / "cgx2.toml").write_text(
        'schema_version = 2\nname = "libtorch_pinned"\nentry = "src/main.cg"\n'
        'sources = ["src/main.cg"]\nbinding_lock = "cgx2-bind.lock.json"\n',
        encoding="utf-8",
    )
    write_json(package / "cgx2-bind.lock.json", {
        "schema_version": 2, "package": "libtorch_pinned", "bindings": [binding],
    })
    write_json(package / "cgx2-libtorch-closure.lock.json", {
        "schema_version": 1, "libraries": closure,
    })
    write_json(package / "cgx2-libtorch-evidence.json", {
        "schema_version": 1, "facade_sha256": facade_entry["sha256"],
        "compiler": str(compiler), "cxx11_abi": int(args.cxx11_abi),
        "cxx11_abi_source": "configured flags" if explicit_abi else "compiler default",
        "cmake_cxx_flags": cmake_flags,
        "torch_cxx_flags": args.torch_cxx_flags, "torch_version": args.version,
        "platform": {"os": "linux", "arch": platform.machine()},
        "libtorch_root": str(root), "build_dir": str(build_dir),
        "build_command": args.build_command,
        "prepare_command": shlex.join(sys.argv),
        "libraries": sorted(discovered.values(), key=lambda item: item["name"]),
        "total_bytes": total,
    })
    print(package)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("facade", "libtorch-root", "build-dir", "package", "version",
                 "compiler", "cxx11-abi", "torch-cxx-flags", "build-command"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--lib-dir", action="append", default=[],
                        help="exact dependency search directory; repeat for system ELF directories")
    parser.add_argument("--select-dependency", action="append", default=[],
                        help="resolve a duplicate as NAME=/canonical/path/to/ELF")
    parser.add_argument("--matrix", action="store_true",
                        help="pin additive two-dimensional float64 matrix API v2 exports")
    parser.add_argument("--matrix-f32", action="store_true",
                        help="pin additive CPU float32 matrix API v3 plus v1/v2 exports")
    args = parser.parse_args()
    try:
        prepare(args)
    except (OSError, UnicodeError, subprocess.TimeoutExpired, ValueError) as error:
        parser.exit(1, f"preparation rejected: {error}\n")


if __name__ == "__main__":
    main()
