#!/usr/bin/env python3
"""Execute versioned release examples with caller-pinned tools (stdlib only)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import selectors
import time

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SUITE = ROOT / "docs/first-release/expectations.json"
MAX_BYTES = 1024 * 1024
SHA = re.compile(r"[0-9a-f]{64}\Z")
NAME = re.compile(r"[a-z][a-z0-9-]{0,79}\Z")
TOOL_REF = re.compile(r"\{tool:([a-z][a-z0-9-]{0,79})\}")


class Invalid(ValueError):
    pass


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            h.update(block)
    return h.hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Invalid(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path):
    if path.stat().st_size > MAX_BYTES:
        raise Invalid("JSON exceeds 1 MiB")
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object,
                          parse_constant=lambda value: fail(f"invalid number: {value}"))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise Invalid(f"invalid JSON: {error}") from error


def fail(message):
    raise Invalid(message)


def fields(value, required, optional=()):
    if not isinstance(value, dict) or set(value) - set(required) - set(optional) or set(required) - set(value):
        fail(f"expected fields {sorted(required)}, optional {sorted(optional)}")


def relative(value):
    if not isinstance(value, str) or not value or "\\" in value:
        fail("expected a nonempty relative POSIX path")
    path = Path(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in value.split("/")):
        fail(f"unsafe relative path: {value}")
    return path


def no_symlinks(path):
    path = Path(os.path.abspath(path))
    for component in (path, *path.parents):
        if component.is_symlink():
            fail(f"symlink path component: {component}")
    return path


def destination(value, fixtures):
    if ".." in Path(value).parts:
        fail("report path cannot contain '..'")
    path = no_symlinks(value)
    if path.exists() or not path.parent.is_dir():
        fail("report must be a new file in an existing directory")
    for protected in (fixtures.resolve(), ROOT / "docs/first-release", ROOT / "tools/docs-check"):
        if path == protected or protected in path.parents:
            fail("report cannot overwrite checker or fixture inputs")
    return path


def validate_match(value):
    if not isinstance(value, dict) or len(value) != 1:
        fail("stream expectation needs exactly one of equals, contains, json")
    key, wanted = next(iter(value.items()))
    if key == "equals" and isinstance(wanted, str):
        return
    if key == "contains" and isinstance(wanted, list) and wanted and all(isinstance(x, str) and x for x in wanted):
        return
    if key == "json" and isinstance(wanted, dict) and wanted:
        return
    fail("invalid stream expectation")


def load_suite(path):
    suite = load_json(path)
    fields(suite, {"schema_version", "baseline", "fixture_root", "cases"})
    if type(suite["schema_version"]) is not int or suite["schema_version"] != 1:
        fail("unsupported suite schema")
    if not isinstance(suite["baseline"], str) or not re.fullmatch(r"[0-9a-f]{40}", suite["baseline"]):
        fail("baseline must be a full Git commit")
    fixtures = no_symlinks(path.parent / relative(suite["fixture_root"]))
    if not fixtures.is_dir():
        fail("fixture root is missing")
    # Do not follow source symlinks into other lanes or the host filesystem.
    for entry in fixtures.rglob("*"):
        if entry.is_symlink() or not (entry.is_file() or entry.is_dir()):
            fail(f"unsupported fixture entry: {entry}")
    cases = suite["cases"]
    if not isinstance(cases, list) or not cases or len(cases) > 200:
        fail("cases must contain 1..200 entries")
    ids = set()
    for case in cases:
        fields(case, {"id", "requirements", "disposition", "scope", "steps"}, {"reason", "handoff"})
        if not isinstance(case["id"], str) or not NAME.fullmatch(case["id"]) or case["id"] in ids:
            fail("invalid or duplicate case id")
        ids.add(case["id"])
        if not isinstance(case["requirements"], list) or not case["requirements"] or any(
                not isinstance(r, str) or not re.fullmatch(r"R(0[1-9]|1[0-5])", r) for r in case["requirements"]):
            fail("invalid requirement references")
        if case["scope"] not in ("source", "cli", "library-only", "external"):
            fail("invalid scope")
        if case["disposition"] not in ("implemented", "expected-unsupported", "future", "external"):
            fail("invalid disposition")
        if not isinstance(case["steps"], list) or len(case["steps"]) > 20:
            fail("steps must be a bounded list")
        if case["disposition"] in ("future", "external"):
            if case["steps"] or not isinstance(case.get("reason"), str) or not case["reason"] or not isinstance(case.get("handoff"), str) or not case["handoff"]:
                fail("reserved/external cases need no guessed steps, a reason and handoff")
        elif not case["steps"]:
            fail("runnable case requires steps")
        if case["disposition"] == "expected-unsupported" and not any(s.get("exit", 0) != 0 for s in case["steps"] if isinstance(s, dict)):
            fail("unsupported case must assert a nonzero exit")
        for step in case["steps"]:
            fields(step, {"tool", "args", "exit", "stdout", "stderr"}, {"diagnostics"})
            if not isinstance(step["tool"], str) or not NAME.fullmatch(step["tool"]):
                fail("invalid tool name")
            if not isinstance(step["args"], list) or any(not isinstance(a, str) or "\0" in a for a in step["args"]):
                fail("args must be strings without NUL")
            for arg in step["args"]:
                remainder = TOOL_REF.sub("", arg.replace("{work}", ""))
                if "{" in remainder or "}" in remainder:
                    fail("unknown argument placeholder")
            if type(step["exit"]) is not int or not 0 <= step["exit"] <= 255:
                fail("invalid expected exit")
            validate_match(step["stdout"])
            validate_match(step["stderr"])
            if "diagnostics" in step:
                if not isinstance(step["diagnostics"], list) or not step["diagnostics"]:
                    fail("diagnostics must be a nonempty list")
                for diag in step["diagnostics"]:
                    fields(diag, {"code", "file", "span"})
                    relative(diag["file"])
                    if not isinstance(diag["code"], str) or not diag["code"]:
                        fail("invalid diagnostic code")
                    span = diag["span"]
                    if span is not None:
                        fields(span, {"start", "end"})
                        if any(type(span[k]) is not int for k in span) or not 0 <= span["start"] <= span["end"]:
                            fail("invalid UTF-8 byte span")
    return suite, fixtures


def subset(actual, expected):
    if isinstance(expected, dict):
        return isinstance(actual, dict) and all(k in actual and subset(actual[k], v) for k, v in expected.items())
    return type(actual) is type(expected) and actual == expected


def matches(output, expectation):
    key, wanted = next(iter(expectation.items()))
    if key == "equals":
        return output == wanted
    if key == "contains":
        return all(item in output for item in wanted)
    try:
        return subset(json.loads(output), wanted)
    except (ValueError, RecursionError):
        return False


def observe(command, cwd, timeout, env):
    """Capture bounded output; kill the private process group on timeout/overflow."""
    # Drain both pipes with fixed caps so a child cannot consume unbounded RAM.
    proc = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, start_new_session=True)
    deadline = time.monotonic() + timeout
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    problem = None
    selector = selectors.DefaultSelector()
    for name, pipe in (("stdout", proc.stdout), ("stderr", proc.stderr)):
        selector.register(pipe, selectors.EVENT_READ, name)
    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                problem = "timeout"
                break
            for key, _ in selector.select(min(remaining, 0.1)):
                block = os.read(key.fileobj.fileno(), 65536)
                if not block:
                    selector.unregister(key.fileobj)
                else:
                    buffers[key.data].extend(block)
                    if len(buffers[key.data]) > MAX_BYTES:
                        problem = "output_limit"
                        break
            if problem:
                break
        if not problem:
            try:
                proc.wait(timeout=max(0.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                problem = "timeout"
    finally:
        # Clean up descendants even if the tool exits while leaving children alive.
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        selector.close()
        proc.stdout.close()
        proc.stderr.close()
    return {"exit": proc.returncode, "stdout": bytes(buffers["stdout"][:MAX_BYTES]).decode("utf-8", "replace"),
            "stderr": bytes(buffers["stderr"][:MAX_BYTES]).decode("utf-8", "replace"), "problem": problem}


def assignments(values, label):
    result = {}
    for value in values:
        name, separator, item = value.partition("=")
        if not separator or not NAME.fullmatch(name) or not item or name in result or name == "artifact":
            fail(f"invalid/duplicate {label}: {value}")
        result[name] = item
    return result


def run(args):
    suite_path = Path(args.suite).absolute()
    suite, fixtures = load_suite(suite_path)
    output = destination(args.report, fixtures)
    paths = assignments(args.tool, "tool")
    pins = assignments(args.pin, "pin")
    if args.compiler:
        if "compiler" in paths:
            fail("compiler specified twice")
        paths["compiler"] = args.compiler
    if args.sha256:
        if "compiler" in pins:
            fail("compiler pin specified twice")
        pins["compiler"] = args.sha256
    if any(not SHA.fullmatch(p) for p in pins.values()) or set(pins) - set(paths):
        fail("pins must be lowercase SHA-256 and refer to supplied tools")
    if set(paths) - set(pins):
        fail("every supplied tool needs an independent expected --pin (or --sha256)")
    if not 0 < args.timeout <= 60:
        fail("timeout must be greater than zero and at most 60 seconds")
    work_root = no_symlinks(args.work_root)
    if not work_root.is_dir() or any(p in work_root.parents or p == work_root for p in (fixtures, ROOT / "tools/docs-check")):
        fail("work root must be an existing directory outside checker/fixture inputs")
    env = os.environ.copy()
    for key in list(env):
        if key.startswith("CGX2") or key == "CGX2C_RUSTC":
            del env[key]
    env.update({"LC_ALL": "C", "CARGO_BUILD_JOBS": "2", "CARGO_TARGET_DIR": str(ROOT / "target/release-docs"),
                "TMPDIR": str(work_root), "RUSTUP_TOOLCHAIN": "1.98.1"})
    tools_info = {}
    for name in sorted(paths):
        path = Path(paths[name]).absolute().resolve()
        info = {"path": str(path), "expected_sha256": pins[name], "version": None,
                "version_note": "No common version interface in baseline preview; help banner and executable digest recorded."}
        try:
            if not path.is_file():
                info.update(status="blocked", reason="not_a_regular_file")
                tools_info[name] = info
                continue
            info["sha256"] = digest(path)
            if info["sha256"] != pins[name]:
                info.update(status="failed", reason="identity_mismatch")
            elif not os.access(path, os.X_OK):
                info.update(status="blocked", reason="not_executable")
            else:
                # Preview tools do not implement --version. Preserve their help banner
                # verbatim and use the independently supplied digest as identity.
                info["banner"] = observe([str(path), "--help"], fixtures, args.timeout, env)
                info["status"] = "ready" if digest(path) == pins[name] and not info["banner"]["problem"] else "failed"
        except OSError as error:
            info.update(status="blocked", reason=type(error).__name__)
        tools_info[name] = info
    results = []
    for case in suite["cases"]:
        result = {k: case[k] for k in ("id", "requirements", "disposition", "scope")}
        result["steps"] = []
        if case["disposition"] in ("future", "external"):
            result.update(status="future" if case["disposition"] == "future" else "blocked",
                          reason=case["reason"], handoff=case["handoff"])
            results.append(result)
            continue
        needed = {s["tool"] for s in case["steps"]} - {"artifact"}
        needed.update(name for s in case["steps"] for a in s["args"] for name in TOOL_REF.findall(a))
        unavailable = sorted(t for t in needed if tools_info.get(t, {}).get("status") != "ready")
        if unavailable:
            result.update(status="failed" if any(tools_info.get(t, {}).get("status") == "failed" for t in unavailable) else "blocked",
                          reason="tool identity/prerequisite unavailable", tools=unavailable)
            results.append(result)
            continue
        result["status"] = "passed"
        with tempfile.TemporaryDirectory(prefix="docs-check-", dir=work_root) as temporary:
            work = Path(temporary)
            shutil.copytree(fixtures, work / "inputs")
            def normalize(value):
                value = value.replace(str(work), "$WORK")
                for tool_name in sorted(tools_info):
                    value = value.replace(tools_info[tool_name]["path"], f"$TOOL:{tool_name}")
                return value
            for step in case["steps"]:
                tool_name = step["tool"]
                if tool_name == "artifact":
                    executable = work / "program"
                    if not executable.is_file() or executable.is_symlink():
                        result.update(status="failed", reason="missing generated executable")
                        break
                    identity = None
                else:
                    executable = Path(tools_info[tool_name]["path"])
                    identity = digest(executable)
                    if identity != pins[tool_name]:
                        result.update(status="failed", reason="tool changed before invocation")
                        break
                expanded = [TOOL_REF.sub(lambda m: tools_info[m[1]]["path"], a.replace("{work}", str(work))) for a in step["args"]]
                command = [str(executable), *expanded]
                try:
                    observation = observe(command, work / "inputs", args.timeout, env)
                except OSError as error:
                    result.update(status="blocked", reason=type(error).__name__)
                    break
                observation.update(command=[normalize(a) for a in command])
                if identity is None:
                    # Temporary native outputs embed build roots and are not byte
                    # reproducible. Do not put a changing hash in the stable report.
                    observation["executable_origin"] = "generated by earlier step in this case; output binary identity not attested"
                else:
                    observation["executable_sha256"] = identity
                for stream in ("stdout", "stderr"):
                    observation[stream] = normalize(observation[stream])
                problems = []
                if observation["problem"]:
                    problems.append(observation["problem"])
                if observation["exit"] != step["exit"]:
                    problems.append("exit")
                for stream in ("stdout", "stderr"):
                    if not matches(observation[stream], step[stream]):
                        problems.append(stream)
                if "diagnostics" in step:
                    try:
                        diagnostics = [json.loads(line) for line in observation["stderr"].splitlines()]
                        observation["diagnostics"] = diagnostics
                        if len(diagnostics) != len(step["diagnostics"]) or any(not subset(a, b) for a, b in zip(diagnostics, step["diagnostics"])):
                            problems.append("diagnostics")
                    except (ValueError, RecursionError):
                        problems.append("diagnostic_json")
                if tool_name != "artifact" and digest(executable) != pins[tool_name]:
                    problems.append("identity_changed")
                observation["mismatches"] = problems
                result["steps"].append(observation)
                if problems:
                    result["status"] = "failed"
                    break
        results.append(result)
    counts = {status: sum(r["status"] == status for r in results) for status in ("passed", "failed", "blocked", "future")}
    report = {"schema_version": 1, "baseline": suite["baseline"], "suite_sha256": digest(suite_path),
              "fixture_sha256": {p.relative_to(fixtures).as_posix(): digest(p) for p in sorted(fixtures.rglob("*")) if p.is_file()},
              "tools": tools_info, "results": results, "counts": counts,
              "acceptance_complete": False, "scope": "documentation fixtures; no global release gate is closed"}
    # Exclusive publication: no overwrite, including a destination created during execution.
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=output.parent, delete=False) as stream:
        staging = Path(stream.name)
        json.dump(report, stream, sort_keys=True, indent=2, ensure_ascii=False, allow_nan=False)
        stream.write("\n")
    try:
        no_symlinks(output)
        os.link(staging, output)
    finally:
        staging.unlink()
    print(json.dumps(counts, sort_keys=True))
    print(f"report: {output}")
    identity_failure = any(i["status"] == "failed" for i in tools_info.values())
    if counts["failed"] or identity_failure:
        return 1
    runnable_blocked = any(r["status"] == "blocked" and r["disposition"] not in ("external", "future") for r in results)
    if runnable_blocked or (not args.runnable_only and (counts["blocked"] or counts["future"])):
        return 2
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", default=str(DEFAULT_SUITE))
    parser.add_argument("--compiler", help="caller-supplied cgx2 executable; never discovered on PATH")
    parser.add_argument("--sha256", help="independently approved compiler SHA-256")
    parser.add_argument("--tool", action="append", default=[], metavar="NAME=PATH")
    parser.add_argument("--pin", action="append", default=[], metavar="NAME=SHA256")
    parser.add_argument("--report", required=True, help="new JSON output, parent directory must exist")
    parser.add_argument("--work-root", default=os.environ.get("TMPDIR", tempfile.gettempdir()))
    parser.add_argument("--timeout", type=float, default=20.0, help="per process seconds, 0 < N <= 60")
    parser.add_argument("--runnable-only", action="store_true", help="allow reserved/external cases to remain incomplete (still reported)")
    args = parser.parse_args(argv)
    try:
        return run(args)
    except (Invalid, OSError, ValueError) as error:
        print(f"docs-check: {error}", file=__import__("sys").stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
