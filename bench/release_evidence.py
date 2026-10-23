#!/usr/bin/env python3
"""Collect, import, verify and report CGx2 evidence. See RELEASE-EVIDENCE.md."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from release_gates import ADAPTERS, GATES, TIMING

ROOT = Path(__file__).resolve().parents[1]
BENCH = ROOT / "bench"
LIMIT = 16 * 1024 * 1024
ENV_KEYS = ("PATH", "LANG", "LC_ALL", "RUSTUP_TOOLCHAIN", "LIBRARY_PATH",
            "LD_LIBRARY_PATH", "CARGO_HOME", "RUSTUP_HOME")
STATUSES = {"passed", "failed", "blocked", "unrun"}


class Invalid(ValueError):
    """Invalid or incomplete evidence, never a passing observation."""


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    data = Path(path).read_bytes()
    if len(data) > LIMIT:
        raise Invalid("JSON size limit")
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise Invalid("duplicate JSON field: " + key)
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(Invalid(value)))


def digest_name(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def blob(directory, data):
    key = sha(data)
    path = directory / "blobs" / key
    path.parent.mkdir(exist_ok=True)
    if not path.exists():
        path.write_bytes(data)
    return key


def get_blob(directory, key):
    if not digest_name(key):
        raise Invalid("invalid blob digest")
    path = directory / "blobs" / key
    if (directory / "blobs").is_symlink() or path.is_symlink() or not path.is_file():
        raise Invalid("missing/unsafe artifact: " + key)
    data = path.read_bytes()
    if sha(data) != key:
        raise Invalid("tampered artifact: " + key)
    return data


def json_blob(directory, key):
    # Decode only bytes already checked against their content address.
    get_blob(directory, key)
    return read_json(directory / "blobs" / key)


def snapshot(directory, paths):
    rows = []
    for label, path in sorted(paths):
        path = Path(path).resolve(strict=True)
        if not path.is_file():
            raise Invalid("snapshot needs regular files")
        rows.append({"name": label, "sha256": blob(directory, path.read_bytes())})
    if not rows:
        raise Invalid("empty provenance snapshot")
    return blob(directory, canonical(rows))


def environment():
    env = {key: os.environ[key] for key in ENV_KEYS if key in os.environ}
    env.update(CARGO_BUILD_JOBS="2", CARGO_TARGET_DIR=str(ROOT / "target/release-evidence"),
               TMPDIR=str(ROOT / "target/tmp-evidence"), RUSTUP_TOOLCHAIN="1.98.1",
               PYTHONDONTWRITEBYTECODE="1")
    for key in ("CARGO_TARGET_DIR", "TMPDIR"):
        Path(env[key]).mkdir(parents=True, exist_ok=True)
    return env


def provenance(directory, compiler, env):
    tracked = subprocess.run(["git", "ls-files", "-z", "crates", "Cargo.toml", "Cargo.lock"],
                             cwd=ROOT, check=True, capture_output=True).stdout.decode().split("\0")
    sources = [(name, ROOT / name) for name in tracked if name]
    source = snapshot(directory, sources)
    tools = snapshot(directory, [(str(p.relative_to(ROOT)), p) for p in BENCH.rglob("*.py")
                                 if "target" not in p.relative_to(BENCH).parts])
    tool_identity = snapshot(directory, [("python", Path(sys.executable))])
    compiler_ref = snapshot(directory, [("compiler", compiler)]) if compiler else None
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout.strip()
    host = {"environment": env, "platform": platform.platform(), "python": sys.version,
            "cpu_count": os.cpu_count(), "load": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
            "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None}
    return {"source": source, "tools": tools, "interpreter": tool_identity,
            "compiler": compiler_ref, "environment": blob(directory, canonical(host)), "git_head": head}


def execute(argv, cwd, env, work, timeout):
    """Bound trusted harness/owner processes; candidate code goes through score only."""
    executable = shutil.which(argv[0], path=env.get("PATH"))
    if not executable:
        raise FileNotFoundError(argv[0])
    with (work / "stdout").open("wb") as out, (work / "stderr").open("wb") as err:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True)
        started = time.monotonic()
        reason = None
        try:
            while process.poll() is None:
                if time.monotonic() - started > timeout:
                    reason = "timeout"
                if out.tell() > LIMIT or err.tell() > LIMIT:
                    reason = "output_limit"
                if reason:
                    break
                time.sleep(0.02)
        finally:
            # Also remove descendants left behind after a trusted runner exits.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        if out.tell() > LIMIT or err.tell() > LIMIT:
            reason = "output_limit"
    return {"argv": argv, "cwd": str(cwd), "exit_code": process.returncode, "reason": reason,
            "executable_sha256": sha(Path(executable).read_bytes()),
            "wall_seconds": time.monotonic() - started}, (work / "stdout").read_bytes()[:LIMIT], (work / "stderr").read_bytes()[:LIMIT]


def artifacts(directory, entries, base):
    result = []
    for item in entries:
        if set(item) != {"path", "sha256"} or not digest_name(item["sha256"]):
            raise Invalid("artifact requires path and SHA-256")
        path = (base / item["path"]).resolve(strict=True)
        data = path.read_bytes()
        if sha(data) != item["sha256"]:
            raise Invalid("input artifact hash mismatch: " + str(path))
        result.append({"name": item["path"], "sha256": blob(directory, data)})
    return result


def input_artifacts(directory, adapter, args):
    """Archive study private material too; a release bundle is NEVER model context."""
    rows = []
    paths = set()
    for index, arg in enumerate(args[:-1]):
        if arg not in {"--pack", "--run", "--trace", "--export", "--candidate-workspace", "--build-provenance"}:
            continue
        path = Path(args[index + 1]).resolve(strict=True)
        if adapter == "task-study" and arg in {"--pack", "--run"}:
            path = path.parent
        paths.add(path)
    total = 0
    for root in sorted(paths):
        for path in sorted(root.rglob("*")) if root.is_dir() else [root]:
            if path.is_symlink():
                raise Invalid("symlink in evidence input snapshot")
            if not path.is_file():
                continue
            size = path.stat().st_size
            total += size
            if total > 512 * 1024 * 1024:
                raise Invalid("study input snapshot exceeds 512 MiB; stage a minimal private pack")
            rows.append({"name": str(path), "sha256": blob(directory, path.read_bytes())})
    return rows


def validate_job(job):
    if not isinstance(job, dict) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", job.get("id", "")):
        raise Invalid("invalid job ID")
    if set(job) - {"id", "adapter", "gate", "args", "artifacts", "timeout_seconds"}:
        raise Invalid("unknown job field")
    adapter = job.get("adapter")
    if adapter != "owner" and adapter not in ADAPTERS:
        raise Invalid("unknown adapter")
    gate = job.get("gate") if adapter == "owner" else ADAPTERS[adapter][1]
    if gate not in GATES or (adapter == "owner" and gate in {v[1] for v in ADAPTERS.values()}):
        raise Invalid("gate requires its existing engine")
    args = job.get("args", [])
    if not isinstance(args, list) or any(not isinstance(arg, str) or "\0" in arg for arg in args):
        raise Invalid("args must be an argv array")
    if any(arg == "--out" or arg.startswith("--out=") for arg in args):
        raise Invalid("output path is pipeline-owned")
    if adapter == "owner" and not args:
        raise Invalid("owner runner argv required")
    if "gate" in job and job["gate"] != gate:
        raise Invalid("adapter/gate mismatch")
    timeout = job.get("timeout_seconds", 180)
    if type(timeout) not in (int, float) or not math.isfinite(timeout) or not 0 < timeout <= 7200:
        raise Invalid("timeout must be positive and <=7200 seconds")
    return gate


def run_job(directory, job, env, quiet):
    gate = validate_job(job)
    row = {"id": job["id"], "adapter": job["adapter"], "gate": gate,
           "command": None, "result": None, "artifacts": [], "review": None, "blocker": None}
    timing = job["adapter"] in TIMING or gate in TIMING
    if timing and (not quiet or not hasattr(os, "getloadavg") or os.getloadavg()[0] > 0.5):
        row["blocker"] = "quiet_host_required: operator --quiet-host and one-minute load <=0.5; queue manually"
        return row
    row["artifacts"] = artifacts(directory, job.get("artifacts", []), ROOT)
    try:
        captured_inputs = input_artifacts(directory, job["adapter"], job.get("args", []))
        row["artifacts"] += captured_inputs
    except FileNotFoundError as error:
        row["blocker"] = "input_unavailable: " + str(error)
        return row
    with tempfile.TemporaryDirectory(dir=env["TMPDIR"], prefix="evidence-") as temp:
        work = Path(temp)
        adapter = job["adapter"]
        output = work / "result.json"
        if adapter == "owner":
            argv = job["args"]
        else:
            argv = [sys.executable, str(BENCH / ADAPTERS[adapter][0])]
            if adapter == "isolation-probe":
                argv += ["--probe"]
            if adapter == "task-study":
                argv += ["report"]
            argv += job.get("args", [])
            if adapter in {"native-runtime", "libtorch-runtime", "task-study"}:
                argv += ["--out", str(output)]
            if adapter == "public-export":
                argv += ["--out", str(work / "public")]
        try:
            command, stdout, stderr = execute(argv, ROOT, env, work, job.get("timeout_seconds", 180))
        except (FileNotFoundError, PermissionError) as error:
            row["blocker"] = "runner_unavailable: " + str(error)
            return row
        command.update(stdout=blob(directory, stdout), stderr=blob(directory, stderr),
                       environment=blob(directory, canonical(env)))
        executable = shutil.which(argv[0], path=env.get("PATH"))
        if blob(directory, Path(executable).read_bytes()) != command["executable_sha256"]:
            raise Invalid("runner executable changed during execution")
        row["command"] = command
        if captured_inputs != input_artifacts(directory, adapter, job.get("args", [])):
            raise Invalid("evidence inputs changed during execution")
        if output.exists():
            row["result"] = blob(directory, output.read_bytes())
        elif stdout.strip():
            try:
                json.loads(stdout)
                row["result"] = blob(directory, stdout)
            except (ValueError, UnicodeDecodeError):
                pass
        if adapter == "public-export" and (work / "public").is_dir():
            # Keep only the public export; never expose these bundles to models.
            row["artifacts"] += [{"name": "public/" + str(p.relative_to(work / "public")),
                                  "sha256": blob(directory, p.read_bytes())}
                                 for p in sorted((work / "public").rglob("*")) if p.is_file()]
        row["artifacts"] += [{"name": "runner/" + str(p.relative_to(work)),
                              "sha256": blob(directory, p.read_bytes())}
                             for p in sorted(work.rglob("*"))
                             if p.is_file() and p not in {work / "stdout", work / "stderr", output}
                             and not p.is_relative_to(work / "public")]
    return row


def verify(directory, expected=None):
    directory = Path(directory)
    path = directory / "manifest.json"
    if directory.is_symlink() or path.is_symlink():
        raise Invalid("symlink bundle")
    manifest = read_json(path)
    key = sha(canonical(manifest))
    if directory.name != key or (expected and expected != key) or path.read_bytes() != canonical(manifest):
        raise Invalid("bundle content address/expected digest mismatch")
    if set(manifest) != {"schema_version", "purpose", "operator", "provenance", "jobs", "imports", "plan"}:
        raise Invalid("exact v1 manifest fields required")
    if manifest["schema_version"] != 1 or manifest["purpose"] not in {"fixture", "release"} or not manifest["operator"]:
        raise Invalid("invalid bundle schema/purpose/operator")
    plan = json_blob(directory, manifest["plan"])
    prov = manifest["provenance"]
    if set(prov) != {"source", "tools", "interpreter", "compiler", "environment", "git_head"}:
        raise Invalid("incomplete provenance")
    for field in ("source", "tools", "interpreter", "compiler"):
        if field == "compiler" and prov[field] is None:
            continue
        files = json_blob(directory, prov[field])
        if not isinstance(files, list) or not files:
            raise Invalid("missing provenance files")
        for item in files:
            if set(item) != {"name", "sha256"} or not isinstance(item["name"], str):
                raise Invalid("invalid provenance file record")
            get_blob(directory, item["sha256"])
    host = json_blob(directory, prov["environment"])
    if not isinstance(host, dict) or not isinstance(host.get("environment"), dict):
        raise Invalid("environment provenance missing")
    ids = set()
    for row in manifest["jobs"]:
        if set(row) != {"id", "adapter", "gate", "command", "result", "artifacts", "review", "blocker"}:
            raise Invalid("exact v1 observation fields required")
        validate_job({"id": row["id"], "adapter": row["adapter"], "gate": row["gate"],
                      "args": ["recorded-owner"] if row["adapter"] == "owner" else []})
        if row["id"] in ids:
            raise Invalid("duplicate observation ID")
        ids.add(row["id"])
        if row["command"] is None:
            if not row["blocker"] or row["result"] is not None:
                raise Invalid("missing execution record")
        else:
            cmd = row["command"]
            if (set(cmd) != {"argv", "cwd", "exit_code", "reason", "executable_sha256", "wall_seconds", "stdout", "stderr", "environment"} or
                    not isinstance(cmd.get("argv"), list) or not cmd["argv"] or
                    any(not isinstance(arg, str) for arg in cmd["argv"]) or
                    type(cmd.get("exit_code")) is not int or not digest_name(cmd.get("executable_sha256")) or
                    type(cmd.get("wall_seconds")) not in (int, float) or
                    not math.isfinite(cmd["wall_seconds"]) or cmd["wall_seconds"] < 0 or
                    cmd.get("reason") not in {None, "timeout", "output_limit"} or not cmd.get("cwd")):
                raise Invalid("incomplete command provenance")
            for field in ("stdout", "stderr", "environment"):
                get_blob(directory, cmd[field])
            get_blob(directory, cmd["executable_sha256"])
            if row["result"]:
                json_blob(directory, row["result"])
        for item in row["artifacts"]:
            get_blob(directory, item["sha256"])
        if row["review"]:
            review = json_blob(directory, row["review"])
            if (set(review) != {"schema_version", "reviewer_id", "operator_id", "decision", "result_sha256", "subject_sha256", "notes"}
                    or review["schema_version"] != 1 or not review["reviewer_id"] or not review["notes"]
                    or review["reviewer_id"] == review["operator_id"] or review["operator_id"] != manifest["operator"]
                    or review["result_sha256"] != row["result"] or review["subject_sha256"] != review_subject(manifest, row)
                    or review["decision"] not in {"accepted", "rejected"}):
                raise Invalid("review must independently bind the exact result and operator")
    for imported in manifest["imports"]:
        if not digest_name(imported):
            raise Invalid("invalid import digest")
        json_blob(directory, imported)
    if (plan.get("purpose") != manifest["purpose"] or plan.get("operator") != manifest["operator"]
            or any(job["id"] not in ids for job in plan.get("jobs", []))):
        raise Invalid("registered observation missing from bundle")
    return manifest


def review_subject(manifest, row):
    """Bind review to raw artifacts/commands and candidate, across worker imports."""
    return sha(canonical({"observation": {k: v for k, v in row.items() if k != "review"},
                          "candidate": {k: manifest["provenance"][k] for k in ("compiler", "source", "tools")},
                          "operator": manifest["operator"], "purpose": manifest["purpose"]}))


def assessment(directory, manifest, row):
    """Never infer full feature/resource acceptance from a narrow observation."""
    if row["blocker"]:
        return "blocked", row["blocker"]
    cmd = row["command"]
    if cmd["reason"]:
        return "failed", "runner " + cmd["reason"]
    if not row["result"]:
        return "failed", "runner produced no JSON evidence"
    data = json_blob(directory, row["result"])
    if not isinstance(data, dict):
        return "failed", "result must be an object"
    status = data.get("status")
    if status in {"unsupported_isolation", "unavailable", "unsupported"}:
        return "blocked", data.get("reason", status)
    if row["adapter"] == "task-study" and any(
            finding.get("severity") == "invalid" for finding in data.get("summary", {}).get("findings", [])):
        return "failed", "study rejected invalid provenance; see intake findings"
    if status in {"insufficient_evidence", "unrun", "unknown", "blocked"}:
        return "blocked", "incomplete study or external prerequisites; see raw result"
    if cmd["exit_code"] != 0:
        return "failed", "runner nonzero exit: " + str(cmd["exit_code"])
    adapter = row["adapter"]
    if adapter == "isolation-probe":
        return "blocked", "capability probe alone cannot pass adversarial selftest"
    if adapter == "task-study":
        if status != "ready":
            return "blocked", "holdout intake not ready"
        if data.get("release_verdict") == "gates_failed":
            return "failed", "paired model gates failed"
        if data.get("release_verdict") != "gates_met":
            return "blocked", "paired confidence intervals do not establish all gates"
        pack_path = cmd["argv"][cmd["argv"].index("--pack") + 1]
        pinned_pack = next((a for a in row["artifacts"] if a["name"] == str(Path(pack_path).resolve())), None)
        if not pinned_pack:
            return "failed", "private pack snapshot missing"
        protocol = json_blob(directory, pinned_pack["sha256"])["protocol"]
        compiler_snapshot = manifest["provenance"]["compiler"]
        if compiler_snapshot is None:
            return "blocked", "compiler provenance missing"
        compiler_hash = json_blob(directory, compiler_snapshot)[0]["sha256"]
        if (protocol["compiler_sha256"] != compiler_hash or
                protocol["cgx2_source_sha256"] != manifest["provenance"]["source"] or
                protocol["tools_sha256"] != manifest["provenance"]["tools"]):
            return "failed", "study does not match sealed compiler/source/tool candidate"
    elif adapter == "owner":
        if (set(data) != {"schema_version", "gate", "status", "scope", "limitations", "checks", "evidence_sha256"} or
                data.get("schema_version") != 1 or data.get("gate") != row["gate"] or
                status not in STATUSES or not isinstance(data.get("scope"), dict) or
                set(data["scope"]) != {"level", "supported", "unsupported"} or
                data["scope"]["level"] not in {"source", "library", "harness"} or
                not isinstance(data["scope"]["supported"], list) or not data["scope"]["supported"] or
                not isinstance(data["scope"]["unsupported"], list) or not isinstance(data.get("limitations"), list)):
            return "failed", "owner assessment v1 schema required"
        requirements = GATES[row["gate"]][2]
        evidence = {item["sha256"] for item in row["artifacts"]}
        if (not data.get("evidence_sha256") or not set(data["evidence_sha256"]).issubset(evidence)
                or set(data.get("checks", {})) != set(requirements)):
            return "failed", "owner checklist/raw evidence incomplete"
        if status != "passed":
            return status, "owner assessment; see raw evidence"
        if any(data["checks"][key] is not True for key in requirements):
            return "failed", "owner acceptance checklist not satisfied"
        if data["scope"]["level"] != "source":
            return "blocked", "library/harness evidence cannot close source-level release acceptance"
    elif adapter == "isolation-selftest":
        import isolation_selftest
        cases = data.get("cases", [])
        if (status != "passed" or {c.get("name") for c in cases} != set(isolation_selftest.CASES)
                or len(cases) != len(isolation_selftest.CASES) or any(c.get("passed") is not True for c in cases)):
            return "failed", "positive/adversarial isolation cases incomplete"
    elif adapter == "score":
        if status != "passed" or not data.get("cases"):
            return "failed", "submission did not pass hidden tests"
    elif adapter == "native-runtime":
        if data.get("valid") is not True or not data.get("pairs") or not data.get("summary") or data.get("errors"):
            return "failed", "native observation invalid/incomplete"
    elif adapter == "libtorch-runtime":
        if status != "valid" or not data.get("runs") or not data.get("cases") or data.get("errors"):
            return "failed", "LibTorch observation invalid/incomplete"
    elif adapter == "public-export":
        if not row["artifacts"]:
            return "failed", "public export artifacts missing"
    elif adapter == "syntax-trace":
        if status != "structurally_valid_not_proof_of_authenticity":
            return "failed", "syntax trace intake failed"
    if manifest["purpose"] == "fixture":
        return "unrun", "fixture exercises plumbing; excluded from release evidence"
    if manifest["provenance"]["compiler"] is None:
        return "blocked", "compiler provenance missing"
    if not row["review"]:
        return "blocked", "independent review pending"
    if json_blob(directory, row["review"])["decision"] != "accepted":
        return "failed", "independent review rejected result"
    return "passed", "reviewed bounded evidence; see scope and raw artifacts"


def report(directory, expected=None):
    directory = Path(directory)
    manifest = verify(directory, expected)
    rows = []
    for gate, (requirement, title, checks) in GATES.items():
        observations = []
        for row in manifest["jobs"]:
            if row["gate"] == gate:
                status, reason = assessment(directory, manifest, row)
                data = json_blob(directory, row["result"]) if row["result"] else {}
                scope = data.get("scope") if isinstance(data, dict) else None
                observations.append({"id": row["id"], "status": status, "reason": reason,
                                     "result_sha256": row["result"],
                                     "review_subject_sha256": review_subject(manifest, row),
                                     "scope": scope or {"level": "harness", "supported": [
                                         "bounded " + row["adapter"] + " observation; exact workload in raw artifacts"],
                                         "unsupported": ["broader source semantics or other release gates"]},
                                     "limitations": data.get("limitations", []) if isinstance(data, dict) else []})
        # A later success never conceals a failure or an incomplete registered job.
        states = {r["status"] for r in observations}
        status = next((s for s in ("failed", "blocked", "unrun", "passed") if s in states), "unrun")
        rows.append({"gate": gate, "requirement": requirement, "title": title,
                     "status": status, "required_checks": list(checks), "observations": observations})
    return {"schema_version": 1, "bundle_sha256": Path(directory).name, "purpose": manifest["purpose"],
            "release_status": "passed" if all(r["status"] == "passed" for r in rows) else "not_ready",
            "gates": rows, "trust_boundary": "Hashes prove integrity, not participant/runner/reviewer authenticity; operator must authenticate identities and archive the bundle digest independently."}


def seal(staging, output, manifest):
    key = sha(canonical(manifest))
    destination = Path(output) / key
    if destination.exists():
        raise Invalid("bundle exists; historical results are never overwritten")
    check = staging / key
    check.mkdir()
    shutil.move(str(staging / "blobs"), check / "blobs")
    (check / "manifest.json").write_bytes(canonical(manifest))
    verify(check)
    check.rename(destination)
    return destination


def collect(plan_path, output, quiet=False, imports=()):
    plan = read_json(plan_path)
    if (set(plan) != {"schema_version", "purpose", "operator", "compiler", "jobs"}
            or plan["schema_version"] != 1 or plan["purpose"] not in {"fixture", "release"}
            or not isinstance(plan["operator"], str) or not plan["operator"].strip()):
        raise Invalid("exact v1 plan schema required")
    ids = []
    for job in plan["jobs"]:
        validate_job(job)
        ids.append(job["id"])
    if len(ids) != len(set(ids)):
        raise Invalid("duplicate registered job ID")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    env = environment()
    with tempfile.TemporaryDirectory(prefix=".collect-", dir=output) as temp:
        staging = Path(temp)
        manifest = {"schema_version": 1, "purpose": plan["purpose"], "operator": plan["operator"],
                    "provenance": provenance(staging, plan["compiler"], env), "jobs": [], "imports": [],
                    "plan": blob(staging, canonical(plan))}
        for imported_path in imports:
            imported_path = Path(imported_path)
            imported = verify(imported_path)
            if (imported["purpose"] != manifest["purpose"] or imported["operator"] != manifest["operator"]
                    or any(imported["provenance"][k] != manifest["provenance"][k] for k in ("source", "compiler", "tools"))):
                raise Invalid("import candidate/tool/purpose/operator mismatch")
            # Copy verified blobs; imported observations preserve their command/environment.
            for path in (imported_path / "blobs").iterdir():
                blob(staging, get_blob(imported_path, path.name))
            blob(staging, canonical(imported))
            manifest["jobs"] += imported["jobs"]
            manifest["imports"].append(imported_path.name)
        all_ids = ids + [r["id"] for r in manifest["jobs"]]
        if len(all_ids) != len(set(all_ids)):
            raise Invalid("duplicate imported/registered job ID")
        local_rows = []
        for job in plan["jobs"]:
            if job["adapter"] == "score" and not selftest_ready(staging, local_rows, job):
                row = {"id": job["id"], "adapter": "score", "gate": "submission", "command": None,
                       "result": None, "artifacts": [], "review": None,
                       "blocker": "isolation_selftest_required: run all real cases on this worker/toolchain before scoring"}
            else:
                row = run_job(staging, job, env, quiet)
            local_rows.append(row)
        manifest["jobs"] += local_rows
        after = provenance(staging, plan["compiler"], env)
        if any(after[k] != manifest["provenance"][k] for k in ("source", "tools", "compiler", "interpreter", "git_head")):
            raise Invalid("candidate/source/tools changed during collection")
        return seal(staging, output, manifest)


def selftest_ready(directory, rows, job):
    def root(argv):
        try:
            return str(Path(argv[argv.index("--toolchain-root") + 1]).resolve())
        except (ValueError, IndexError):
            return None
    wanted = root(job.get("args", []))
    if wanted is None:
        return False
    for row in rows:
        if (row["adapter"] == "isolation-selftest" and row["result"] and row["command"]
                and row["command"]["exit_code"] == 0 and row["command"]["reason"] is None
                and root(row["command"]["argv"]) == wanted):
            # Schema/case completeness checks run before the ordinary review gate.
            state, reason = assessment(directory, {"purpose": "fixture"}, row)
            if state == "unrun" and reason.startswith("fixture exercises"):
                return True
    return False


def review_bundle(directory, job_id, receipt, output):
    directory = Path(directory)
    manifest = verify(directory)
    row = next((r for r in manifest["jobs"] if r["id"] == job_id), None)
    if row is None or row["result"] is None or row["review"] is not None:
        raise Invalid("review needs a new result with no existing review")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".review-", dir=output) as temp:
        staging = Path(temp)
        shutil.copytree(directory / "blobs", staging / "blobs")
        blob(staging, canonical(manifest))
        row["review"] = blob(staging, canonical(read_json(receipt)))
        manifest["imports"] += [directory.name]
        # Validate before publishing; a bad receipt never creates a sealed bundle.
        key = sha(canonical(manifest))
        check = staging / key
        check.mkdir()
        shutil.move(str(staging / "blobs"), check / "blobs")
        (check / "manifest.json").write_bytes(canonical(manifest))
        verify(check)
        destination = output / key
        if destination.exists():
            raise Invalid("review bundle exists")
        check.rename(destination)
        return destination


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    run = sub.add_parser("collect")
    run.add_argument("--plan", required=True, type=Path)
    run.add_argument("--out", required=True, type=Path)
    run.add_argument("--quiet-host", action="store_true")
    run.add_argument("--import-bundle", action="append", default=[], type=Path)
    for name in ("verify", "report"):
        command = sub.add_parser(name)
        command.add_argument("--bundle", required=True, type=Path)
        command.add_argument("--expected-sha256")
    review = sub.add_parser("review")
    review.add_argument("--bundle", required=True, type=Path)
    review.add_argument("--job", required=True)
    review.add_argument("--receipt", required=True, type=Path)
    review.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.action == "collect":
            directory = collect(args.plan, args.out, args.quiet_host, args.import_bundle)
            result = {"bundle": str(directory), "report": report(directory)}
        elif args.action == "review":
            directory = review_bundle(args.bundle, args.job, args.receipt, args.out)
            result = {"bundle": str(directory), "report": report(directory)}
        elif args.action == "verify":
            verify(args.bundle, args.expected_sha256)
            result = {"status": "verified", "bundle_sha256": args.bundle.name}
        else:
            result = report(args.bundle, args.expected_sha256)
        print(json.dumps(result, sort_keys=True, indent=2, allow_nan=False))
        return 0 if args.action == "verify" else (0 if result.get("report", result).get("release_status") == "passed" else 2)
    except (OSError, ValueError, KeyError, TypeError, RecursionError, subprocess.SubprocessError) as error:
        print(json.dumps({"status": "invalid_evidence", "reason": str(error)}, sort_keys=True))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
