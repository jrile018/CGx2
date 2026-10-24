#!/usr/bin/env python3
"""Run the unchanged standalone checks with private scratch and an incremental journal.

Only its literal /tmp scratch placement is redirected. Command/result assertions
remain the original helper's; every attempted child is independently persisted.
"""
import hashlib
import json
from pathlib import Path
import runpy
import sys
import tempfile
import traceback

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
import supervisor


def main():
    base, *arguments = sys.argv[1:]
    base = Path(base).resolve()
    base.mkdir(parents=True, exist_ok=True)
    output = Path(arguments[arguments.index("--output-dir") + 1]).resolve()
    journal = output.with_name(output.name + "-journal.json")
    if journal.exists():
        raise SystemExit("refusing to replace " + str(journal))
    helper = HERE.parent / "ordinary-memory-evidence/standalone.py"
    report = {"helper_sha256": hashlib.sha256(helper.read_bytes()).hexdigest(),
              "scratch_redirect": {"from": "/tmp", "to": str(base)}, "commands": []}
    original_temp = tempfile.mkdtemp
    original_supervise = supervisor.supervise

    def save():
        journal.write_text(json.dumps(report, indent=2) + "\n")

    def private_temp(*args, **kwargs):
        if kwargs.get("prefix") == "cgx2-ordinary-memory-standalone-" and kwargs.get("dir") == "/tmp":
            kwargs["dir"] = str(base)
        return original_temp(*args, **kwargs)

    def supervised(command, **kwargs):
        row = {"argv": [str(a) for a in command], "cwd": str(kwargs["cwd"]), "status": "running"}
        report["commands"].append(row)
        save()
        result = original_supervise(command, **kwargs)
        row.update(supervisor.evidence(result))
        row["status"] = "complete"
        save()
        return result

    tempfile.mkdtemp = private_temp
    supervisor.supervise = supervised
    sys.argv = [str(helper)] + arguments
    save()
    try:
        runpy.run_path(str(helper), run_name="__main__")
    except SystemExit as error:
        report["exit"] = error.code
        raise
    except BaseException:
        report["traceback"] = traceback.format_exc()
        raise
    finally:
        tempfile.mkdtemp = original_temp
        supervisor.supervise = original_supervise
        save()


if __name__ == "__main__":
    main()
