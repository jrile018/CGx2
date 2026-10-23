"""Rebuild the tiny blocked FIXTURE bundle; no checks or approvals are simulated."""

from pathlib import Path
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
import release_evidence as evidence


def main():
    output = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(dir=output, prefix=".fixture-") as temp:
        stage = Path(temp)
        text = evidence.blob(stage, b"FIXTURE ONLY: no compiler, study, solver, leak check or review was run.\n")
        files = evidence.blob(stage, evidence.canonical([{"name": "fixture-marker.txt", "sha256": text}]))
        environment = evidence.blob(stage, evidence.canonical({"environment": {"fixture": "not observed"}}))
        job = {"id": "fixture-only", "adapter": "owner", "gate": "leak-check", "args": ["/fixture/no-runner"]}
        plan = {"schema_version": 1, "purpose": "fixture", "operator": "fixture-not-a-person", "compiler": None, "jobs": [job]}
        manifest = {"schema_version": 1, "purpose": "fixture", "operator": plan["operator"],
                    "provenance": {"source": files, "tools": files, "interpreter": files, "compiler": None,
                                   "environment": environment, "git_head": "0" * 40},
                    "jobs": [{"id": job["id"], "adapter": "owner", "gate": "leak-check", "command": None,
                              "result": None, "artifacts": [], "review": None,
                              "blocker": "FIXTURE ONLY: owner runner and all real provenance absent"}],
                    "imports": [], "plan": evidence.blob(stage, evidence.canonical(plan))}
        destination = evidence.seal(stage, output, manifest)
        print(destination)


if __name__ == "__main__":
    main()
