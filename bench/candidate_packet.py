"""Create and validate review/evidence packets without inventing model runs."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import task_evidence


BASE = Path(__file__).resolve().parent / "fixtures/task-candidates/v3"
REQUIRED_TRACE = {
    "task_id", "language", "model_family", "model_version", "prompt_budget_tokens",
    "sampling", "trial_id", "run_id", "workspace_sha256", "passed",
    "input_tokens", "generated_tokens", "total_tokens", "latency_ms",
    "compile_ms", "run_ms", "paired_baseline_run_id",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def read_material(base=BASE):
    pack = json.loads((base / "pack.json").read_text(encoding="utf-8"))
    cards = json.loads((base / "task-cards.json").read_text(encoding="utf-8"))
    ids = [c["task_id"] for c in cards["cards"]]
    if len(ids) != len(set(ids)) or set(ids) != {t["task_id"] for t in pack["tasks"]}:
        raise ValueError("review cards do not match candidate tasks")
    fields = ("user_need", "allowed_files", "public_api", "completion_criteria",
              "hidden_cases", "edge_cases", "seed_data_provenance", "substantive_reason")
    review_fields = ("ambiguity", "leakage", "difficulty", "corpus_duplication",
                     "decision", "accept_reject_rationale", "reviewer_id", "reviewed_at_utc")
    for card in cards["cards"]:
        if any(not card.get(field) for field in fields) or any(
                field not in card.get("review", {}) for field in review_fields):
            raise ValueError("incomplete task card or reviewer checklist")
    audit = task_evidence.audit(pack, base)
    if any(finding["severity"] == "invalid" for finding in audit["findings"]):
        raise ValueError("candidate material has an invalid pin or fixture")
    return pack, {c["task_id"]: c for c in cards["cards"]}


def make_packet(base=BASE, traces=(), trace_file_sha256=None):
    pack, cards = read_material(base)
    harness_digest = sha((base / "private/accept.py").read_bytes())
    by_task = {}
    for task in pack["tasks"]:
        task_id = task["task_id"]
        card = cards[task_id]
        review = card["review"]
        state = review_state(task, review)
        by_task[task_id] = {
            "task_id": task_id,
            "task_sha256": sha(canonical({"manifest": task, "card": card})),
            "harness_sha256": harness_digest,
            "starter_sha256": task["repository_snapshots"],
            "prompt_sha256": task["prompt_sha256"],
            "review_state": state,
            "run_state": "unrun",
            "claim_status": "not_assessed",
            "trials": [],
        }
    for trace in traces:
        if trace["task_id"] not in by_task:
            raise ValueError("unknown trace task")
        by_task[trace["task_id"]]["trials"].append(trace)
        by_task[trace["task_id"]]["run_state"] = "observed"
    packet = {"schema_version": 1, "kind": "candidate_review_packet",
              "protocol_sha256": sha(canonical(pack["protocol"])),
              "external_trace_file_sha256": trace_file_sha256,
              "tasks": list(by_task.values())}
    validate(packet, base)
    return packet


def review_state(task, review):
    decision = review["decision"]
    if decision not in (None, "accept", "reject"):
        raise ValueError("invalid reviewer decision")
    if decision is not None:
        required = ("ambiguity", "leakage", "difficulty", "corpus_duplication",
                    "accept_reject_rationale", "reviewer_id", "reviewed_at_utc")
        if any(not isinstance(review[field], str) or not review[field].strip() for field in required):
            raise ValueError("review decision needs a complete checklist and rationale")
    if decision == "reject":
        return "rejected"
    return "approved" if decision == "accept" and task["status"] == "frozen" else "pending"


def validate(packet, base=BASE):
    pack, cards = read_material(base)
    if packet.get("schema_version") != 1 or packet.get("kind") != "candidate_review_packet":
        raise ValueError("unknown packet format")
    if packet.get("protocol_sha256") != sha(canonical(pack["protocol"])):
        raise ValueError("protocol digest mismatch")
    tasks = packet.get("tasks")
    if not isinstance(tasks, list) or {t.get("task_id") for t in tasks} != set(cards):
        raise ValueError("packet task set mismatch")
    seen_runs = set()
    all_traces = []
    harness_sha = sha((base / "private/accept.py").read_bytes())
    for entry in tasks:
        task = next(t for t in pack["tasks"] if t["task_id"] == entry["task_id"])
        card = cards[entry["task_id"]]
        expected_state = review_state(task, card["review"])
        if entry.get("task_sha256") != sha(canonical({"manifest": task, "card": card})):
            raise ValueError("task digest mismatch")
        if entry.get("harness_sha256") != harness_sha or task["hidden_test_sha256"] != harness_sha:
            raise ValueError("harness digest mismatch")
        if entry.get("starter_sha256") != task["repository_snapshots"]:
            raise ValueError("starter digest mismatch")
        if entry.get("prompt_sha256") != task["prompt_sha256"]:
            raise ValueError("prompt digest mismatch")
        if entry.get("review_state") != expected_state or entry.get("claim_status") != "not_assessed":
            raise ValueError("review state or claim is invalid")
        trials = entry.get("trials")
        if not isinstance(trials, list) or entry.get("run_state") != ("observed" if trials else "unrun"):
            raise ValueError("unrun/observed state does not match trials")
        for trace in trials:
            if set(trace) != REQUIRED_TRACE or trace["task_id"] != task["task_id"]:
                raise ValueError("trace fields/task mismatch")
            if trace["language"] not in task["applicable_languages"]:
                raise ValueError("trace language not in task")
            for field in ("model_family", "model_version", "trial_id", "run_id"):
                if not isinstance(trace[field], str) or not trace[field].strip():
                    raise ValueError(f"missing {field}")
            if trace["run_id"] in seen_runs:
                raise ValueError("duplicate run ID")
            seen_runs.add(trace["run_id"])
            if not isinstance(trace["sampling"], dict) or not trace["sampling"]:
                raise ValueError("sampling settings required")
            if not isinstance(trace["passed"], bool):
                raise ValueError("pass/fail must be Boolean")
            for field in ("prompt_budget_tokens", "input_tokens", "generated_tokens",
                          "total_tokens", "latency_ms", "compile_ms", "run_ms"):
                value = trace[field]
                if (isinstance(value, bool) or not isinstance(value, (int, float))
                        or not math.isfinite(value) or value < 0):
                    raise ValueError(f"invalid {field}")
            for field in ("prompt_budget_tokens", "input_tokens", "generated_tokens", "total_tokens"):
                if not isinstance(trace[field], int):
                    raise ValueError(f"{field} must be an integer")
            if trace["total_tokens"] != trace["input_tokens"] + trace["generated_tokens"]:
                raise ValueError("token accounting mismatch")
            if trace["input_tokens"] > trace["prompt_budget_tokens"]:
                raise ValueError("prompt budget exceeded")
            digest = trace["workspace_sha256"]
            if not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
                raise ValueError("workspace digest required")
            if trace["language"] == "cgx2" and not trace["paired_baseline_run_id"]:
                raise ValueError("CGx2 trial needs paired baseline link")
            if trace["language"] != "cgx2" and trace["paired_baseline_run_id"] is not None:
                raise ValueError("baseline trial cannot point to another baseline")
            all_traces.append(trace)
    runs = {trace["run_id"]: trace for trace in all_traces}
    for trace in all_traces:
        if trace["language"] != "cgx2":
            continue
        baseline = runs.get(trace["paired_baseline_run_id"])
        if (baseline is None or baseline["task_id"] != trace["task_id"]
                or baseline["language"] != "rust" or baseline["trial_id"] != trace["trial_id"]
                or baseline["model_family"] != trace["model_family"]
                or baseline["model_version"] != trace["model_version"]):
            raise ValueError("paired baseline link does not identify matching Rust trial")
    return True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="write generated packet")
    parser.add_argument("--validate", type=Path, help="validate an existing packet")
    parser.add_argument("--traces", type=Path, help="import externally captured trace JSON list")
    args = parser.parse_args(argv)
    if args.validate:
        validate(json.loads(args.validate.read_text(encoding="utf-8")))
        print("candidate packet valid")
    else:
        traces = json.loads(args.traces.read_text(encoding="utf-8")) if args.traces else []
        packet = make_packet(traces=traces,
                             trace_file_sha256=sha(args.traces.read_bytes()) if args.traces else None)
        rendered = json.dumps(packet, indent=2, sort_keys=True) + "\n"
        if args.out:
            args.out.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")


if __name__ == "__main__":
    main()
