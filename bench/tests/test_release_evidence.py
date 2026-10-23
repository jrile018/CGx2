"""Synthetic temporary receipts test validation, NEVER release observations."""

import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_evidence as evidence
from release_gates import GATES


class ReleaseEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "bundles"
        self.output.mkdir()
        self.env = {"PATH": os.environ["PATH"], "TMPDIR": str(self.root)}

    def staged(self, purpose="fixture", jobs=None, compiler=True):
        staging = self.root / "stage"
        staging.mkdir()
        file = evidence.blob(staging, b"synthetic fixture bytes; not evidence\n")
        snapshot = evidence.blob(staging, evidence.canonical([{"name": "fixture", "sha256": file}]))
        host = evidence.blob(staging, evidence.canonical({"environment": self.env}))
        self.result = {"schema_version": 1, "gate": "leak-check", "status": "passed",
                       "scope": {"level": "source", "supported": ["synthetic test only"], "unsupported": ["all real release claims"]}, "limitations": ["not release evidence"],
                       "checks": {key: True for key in GATES["leak-check"][2]}, "evidence_sha256": [file]}
        command = {"argv": ["synthetic-fixture-runner"], "cwd": str(self.root), "exit_code": 0,
                   "reason": None, "executable_sha256": file, "wall_seconds": 1.0,
                   "stdout": file, "stderr": file, "environment": host}
        row = {"id": "synthetic", "adapter": "owner", "gate": "leak-check", "command": command,
               "result": evidence.blob(staging, evidence.canonical(self.result)),
               "artifacts": [{"name": "fixture", "sha256": file}], "review": None, "blocker": None}
        self.plan = {"schema_version": 1, "purpose": purpose, "operator": "test-operator", "compiler": None,
                     "jobs": [{"id": "synthetic", "adapter": "owner", "gate": "leak-check", "args": ["synthetic-fixture-runner"]}]}
        manifest = {"schema_version": 1, "purpose": purpose, "operator": "test-operator",
                    "provenance": {"source": snapshot, "tools": snapshot, "interpreter": snapshot,
                                   "compiler": snapshot if compiler else None, "environment": host, "git_head": "0" * 40},
                    "jobs": [row] if jobs is None else jobs, "imports": [],
                    "plan": evidence.blob(staging, evidence.canonical(self.plan))}
        return staging, manifest

    def bundle(self, **kwargs):
        staging, manifest = self.staged(**kwargs)
        directory = evidence.seal(staging, self.output, manifest)
        return directory, manifest

    def receipt(self, manifest, **updates):
        receipt = {"schema_version": 1, "reviewer_id": "test-reviewer", "operator_id": "test-operator",
                   "decision": "accepted", "result_sha256": manifest["jobs"][0]["result"],
                   "subject_sha256": evidence.review_subject(manifest, manifest["jobs"][0]), "notes": "synthetic review fixture only"}
        receipt.update(updates)
        path = self.root / "review.json"
        path.write_bytes(evidence.canonical(receipt))
        return path

    def gate(self, directory, name="leak-check"):
        return next(row for row in evidence.report(directory)["gates"] if row["gate"] == name)

    def test_valid_bundle_and_deterministic_report(self):
        directory, _ = self.bundle()
        self.assertEqual(evidence.report(directory), evidence.report(directory))
        self.assertEqual(self.gate(directory)["status"], "unrun")
        self.assertEqual(len(evidence.report(directory)["gates"]), len(GATES))

    def test_fixture_with_review_never_passes(self):
        directory, manifest = self.bundle()
        reviewed = evidence.review_bundle(directory, "synthetic", self.receipt(manifest), self.output)
        self.assertEqual(self.gate(reviewed)["status"], "unrun")

    def test_independent_review_creates_new_bundle(self):
        directory, manifest = self.bundle(purpose="release")
        before = (directory / "manifest.json").read_bytes()
        self.assertEqual(self.gate(directory)["status"], "blocked")
        reviewed = evidence.review_bundle(directory, "synthetic", self.receipt(manifest), self.output)
        self.assertNotEqual(directory, reviewed)
        self.assertEqual(before, (directory / "manifest.json").read_bytes())
        self.assertEqual(self.gate(reviewed)["status"], "passed")
        self.assertEqual(evidence.report(reviewed)["release_status"], "not_ready")

    def test_review_same_identity_rejects(self):
        directory, manifest = self.bundle()
        with self.assertRaises(evidence.Invalid):
            evidence.review_bundle(directory, "synthetic", self.receipt(manifest, reviewer_id="test-operator"), self.output)

    def test_review_wrong_result_rejects(self):
        directory, manifest = self.bundle()
        with self.assertRaises(evidence.Invalid):
            evidence.review_bundle(directory, "synthetic", self.receipt(manifest, result_sha256="a" * 64), self.output)

    def test_review_cannot_overwrite_prior_review(self):
        directory, manifest = self.bundle()
        receipt = self.receipt(manifest)
        reviewed = evidence.review_bundle(directory, "synthetic", receipt, self.output)
        with self.assertRaises(evidence.Invalid):
            evidence.review_bundle(reviewed, "synthetic", receipt, self.output)

    def test_rejected_review_fails(self):
        directory, manifest = self.bundle(purpose="release")
        reviewed = evidence.review_bundle(directory, "synthetic", self.receipt(manifest, decision="rejected"), self.output)
        self.assertEqual(self.gate(reviewed)["status"], "failed")

    def test_missing_compiler_remains_blocked(self):
        directory, manifest = self.bundle(purpose="release", compiler=False)
        reviewed = evidence.review_bundle(directory, "synthetic", self.receipt(manifest), self.output)
        self.assertEqual(self.gate(reviewed)["status"], "blocked")

    def test_tampered_artifact_rejects(self):
        directory, manifest = self.bundle()
        path = directory / "blobs" / manifest["jobs"][0]["result"]
        path.write_text("{}")
        with self.assertRaises(evidence.Invalid):
            evidence.verify(directory)

    def test_missing_artifact_rejects(self):
        directory, manifest = self.bundle()
        (directory / "blobs" / manifest["jobs"][0]["command"]["stdout"]).unlink()
        with self.assertRaises(evidence.Invalid):
            evidence.verify(directory)

    def test_tampered_manifest_rejects(self):
        directory, manifest = self.bundle()
        manifest["operator"] = "other"
        (directory / "manifest.json").write_bytes(evidence.canonical(manifest))
        with self.assertRaises(evidence.Invalid):
            evidence.verify(directory)

    def test_expected_digest_rejects_resealed_substitution(self):
        directory, _ = self.bundle()
        with self.assertRaises(evidence.Invalid):
            evidence.verify(directory, "a" * 64)

    def test_missing_provenance_rejects(self):
        staging, manifest = self.staged()
        del manifest["provenance"]["environment"]
        with self.assertRaises(evidence.Invalid):
            evidence.seal(staging, self.output, manifest)
        self.assertFalse(list(self.output.iterdir()))

    def test_omitted_registered_observation_rejects(self):
        staging, manifest = self.staged(jobs=[])
        with self.assertRaises(evidence.Invalid):
            evidence.seal(staging, self.output, manifest)

    def test_missing_exit_code_rejects(self):
        staging, manifest = self.staged()
        del manifest["jobs"][0]["command"]["exit_code"]
        with self.assertRaises(evidence.Invalid):
            evidence.seal(staging, self.output, manifest)

    def test_duplicate_jobs_reject(self):
        staging, manifest = self.staged()
        manifest["jobs"] *= 2
        with self.assertRaises(evidence.Invalid):
            evidence.seal(staging, self.output, manifest)

    def test_symlink_blob_rejects(self):
        directory, manifest = self.bundle()
        path = directory / "blobs" / manifest["jobs"][0]["result"]
        original = self.root / "original"
        path.rename(original)
        path.symlink_to(original)
        with self.assertRaises(evidence.Invalid):
            evidence.verify(directory)

    def test_owner_evidence_missing_cannot_pass(self):
        staging, manifest = self.staged(purpose="release")
        self.result["evidence_sha256"] = ["a" * 64]
        manifest["jobs"][0]["result"] = evidence.blob(staging, evidence.canonical(self.result))
        directory = evidence.seal(staging, self.output, manifest)
        self.assertEqual(self.gate(directory)["status"], "failed")

    def test_owner_incomplete_checklist_cannot_pass(self):
        staging, manifest = self.staged(purpose="release")
        self.result["checks"]["leak_detection_enabled"] = False
        manifest["jobs"][0]["result"] = evidence.blob(staging, evidence.canonical(self.result))
        directory = evidence.seal(staging, self.output, manifest)
        self.assertEqual(self.gate(directory)["status"], "failed")

    def test_nonzero_exit_cannot_pass(self):
        staging, manifest = self.staged()
        manifest["jobs"][0]["command"]["exit_code"] = 1
        directory = evidence.seal(staging, self.output, manifest)
        self.assertEqual(self.gate(directory)["status"], "failed")

    def test_library_assessment_cannot_close_source_gate(self):
        staging, manifest = self.staged(purpose="release")
        self.result["scope"]["level"] = "library"
        manifest["jobs"][0]["result"] = evidence.blob(staging, evidence.canonical(self.result))
        directory = evidence.seal(staging, self.output, manifest)
        self.assertEqual(self.gate(directory)["status"], "blocked")

    def test_owner_runner_unavailable_is_blocked(self):
        stage = self.root / "stage"
        stage.mkdir()
        result = evidence.run_job(stage, {"id": "missing", "adapter": "owner", "gate": "trusted-solver",
                                         "args": [str(self.root / "not-present")]}, self.env, False)
        self.assertIn("runner_unavailable", result["blocker"])
        self.assertIsNone(result["command"])

    def test_performance_never_runs_without_quiet_host(self):
        stage = self.root / "stage"
        stage.mkdir()
        with patch.object(evidence, "execute", side_effect=AssertionError("must not execute")):
            row = evidence.run_job(stage, {"id": "queued", "adapter": "native-runtime"}, self.env, False)
        self.assertIn("quiet_host_required", row["blocker"])

    def test_busy_host_blocks_even_with_operator_flag(self):
        stage = self.root / "stage"
        stage.mkdir()
        with patch.object(evidence.os, "getloadavg", return_value=(2, 2, 2)), patch.object(evidence, "execute", side_effect=AssertionError):
            row = evidence.run_job(stage, {"id": "queued", "adapter": "native-runtime"}, self.env, True)
        self.assertIn("quiet_host_required", row["blocker"])

    def test_owner_cannot_bypass_existing_scorer(self):
        with self.assertRaises(evidence.Invalid):
            evidence.validate_job({"id": "bypass", "adapter": "owner", "gate": "submission", "args": ["fake"]})

    def test_unknown_adapter_and_gate_reject(self):
        for job in [{"id": "x", "adapter": "unknown"}, {"id": "x", "adapter": "owner", "gate": "invented", "args": ["fake"]}]:
            with self.assertRaises(evidence.Invalid):
                evidence.validate_job(job)

    def test_reserved_output_and_invalid_timeout_reject(self):
        for extra in [{"args": ["--out=elsewhere"]}, {"timeout_seconds": 0}, {"timeout_seconds": float("nan")}, {"args": "shell text"}]:
            with self.assertRaises(evidence.Invalid):
                evidence.validate_job({"id": "x", "adapter": "task-study", **extra})

    def test_duplicate_json_and_nonfinite_reject(self):
        path = self.root / "bad.json"
        for value in ['{"x":1,"x":2}', '{"x":NaN}']:
            path.write_text(value)
            with self.assertRaises(evidence.Invalid):
                evidence.read_json(path)

    def test_actual_timeout_captured(self):
        command, _, _ = evidence.execute([sys.executable, "-c", "import time; time.sleep(10)"], self.root, self.env, self.root, 0.03)
        self.assertEqual(command["reason"], "timeout")
        self.assertNotEqual(command["exit_code"], 0)

    def test_actual_owner_execution_captures_json_and_raw_streams(self):
        stage = self.root / "stage"
        stage.mkdir()
        row = evidence.run_job(stage, {"id": "fixture", "adapter": "owner", "gate": "leak-check",
                                      "args": [sys.executable, "-c", 'print("{\\"status\\":\\"unrun\\"}")']}, self.env, False)
        self.assertEqual(row["command"]["exit_code"], 0)
        self.assertEqual(evidence.json_blob(stage, row["result"])["status"], "unrun")
        evidence.get_blob(stage, row["command"]["executable_sha256"])

    def test_existing_incomplete_study_remains_blocked(self):
        stage = self.root / "stage"
        stage.mkdir()
        row = evidence.run_job(stage, {"id": "candidate", "adapter": "task-study", "args": ["--pack", str(evidence.BENCH / "fixtures/task-candidates/v3/pack.json")]}, self.env, False)
        data = evidence.json_blob(stage, row["result"])
        self.assertEqual(data["status"], "insufficient_evidence")
        self.assertIsNone(data["comparison"])
        self.assertEqual(evidence.assessment(stage, {"purpose": "release"}, row)[0], "blocked")
        self.assertTrue(row["artifacts"])

    def test_one_failure_not_hidden_by_other_pass(self):
        staging, manifest = self.staged(purpose="release")
        other = copy.deepcopy(manifest["jobs"][0])
        other["id"] = "failed"
        other["command"]["exit_code"] = 7
        manifest["jobs"].append(other)
        directory = evidence.seal(staging, self.output, manifest)
        self.assertEqual(self.gate(directory)["status"], "failed")

    def import_plan(self, purpose="fixture"):
        path = self.root / "import.json"
        path.write_bytes(evidence.canonical({"schema_version": 1, "purpose": purpose, "operator": "test-operator", "compiler": None, "jobs": []}))
        return path

    def provenance_copy(self, source, manifest):
        def fake(stage, compiler, env):
            for path in (source / "blobs").iterdir():
                evidence.blob(stage, path.read_bytes())
            return manifest["provenance"]
        return fake

    def test_import_preserves_observations_and_history(self):
        directory, manifest = self.bundle()
        with patch.object(evidence, "provenance", side_effect=self.provenance_copy(directory, manifest)):
            imported = evidence.collect(self.import_plan(), self.output, imports=[directory])
        actual = evidence.verify(imported)
        self.assertEqual(actual["jobs"], manifest["jobs"])
        self.assertEqual(evidence.json_blob(imported, actual["imports"][0]), manifest)
        self.assertEqual(self.gate(imported)["status"], "unrun")

    def test_import_rejects_mixed_candidate(self):
        directory, manifest = self.bundle()
        def changed(stage, compiler, env):
            prov = self.provenance_copy(directory, manifest)(stage, compiler, env).copy()
            prov["source"] = "a" * 64
            return prov
        with patch.object(evidence, "provenance", side_effect=changed), self.assertRaises(evidence.Invalid):
            evidence.collect(self.import_plan(), self.output, imports=[directory])

    def test_review_cannot_be_transplanted_to_changed_raw_evidence(self):
        directory, manifest = self.bundle()
        receipt = self.receipt(manifest)
        manifest["jobs"][0]["command"]["argv"] += ["changed-input"]
        staging = self.root / "changed-stage"
        staging.mkdir()
        import shutil
        shutil.copytree(directory / "blobs", staging / "blobs")
        manifest["jobs"][0]["review"] = evidence.blob(staging, receipt.read_bytes())
        with self.assertRaises(evidence.Invalid):
            evidence.seal(staging, self.output, manifest)

    def test_reviewed_import_preserves_worker_environment_and_review(self):
        directory, manifest = self.bundle(purpose="release")
        reviewed = evidence.review_bundle(directory, "synthetic", self.receipt(manifest), self.output)
        reviewed_manifest = evidence.verify(reviewed)
        with patch.object(evidence, "provenance", side_effect=self.provenance_copy(reviewed, reviewed_manifest)):
            imported = evidence.collect(self.import_plan(purpose="release"), self.output, imports=[reviewed])
        self.assertEqual(self.gate(imported)["status"], "passed")

    def test_import_rejects_fixture_laundering(self):
        directory, manifest = self.bundle()
        with patch.object(evidence, "provenance", side_effect=self.provenance_copy(directory, manifest)), self.assertRaises(evidence.Invalid):
            evidence.collect(self.import_plan(purpose="release"), self.output, imports=[directory])

    def test_import_rejects_duplicate_jobs(self):
        directory, manifest = self.bundle()
        with patch.object(evidence, "provenance", side_effect=self.provenance_copy(directory, manifest)), self.assertRaises(evidence.Invalid):
            evidence.collect(self.import_plan(), self.output, imports=[directory, directory])

    def test_result_and_owner_schemas(self):
        import jsonschema
        directory, manifest = self.bundle()
        for filename, value in [("release-evidence-v1.schema.json", manifest), ("release-owner-v1.schema.json", self.result)]:
            schema = json.loads((evidence.BENCH / "schema" / filename).read_text())
            jsonschema.Draft202012Validator.check_schema(schema)
            jsonschema.validate(value, schema)
        broken = copy.deepcopy(manifest)
        del broken["jobs"][0]["command"]["exit_code"]
        with self.assertRaises(jsonschema.ValidationError):
            jsonschema.validate(broken, json.loads((evidence.BENCH / "schema/release-evidence-v1.schema.json").read_text()))

    def test_invalid_study_provenance_fails_not_blocks(self):
        staging, manifest = self.staged()
        row = manifest["jobs"][0]
        row.update(adapter="task-study", gate="task-study", result=evidence.blob(staging, evidence.canonical({
            "status": "insufficient_evidence", "summary": {"findings": [{"severity": "invalid", "code": "run_provenance"}]}})))
        self.assertEqual(evidence.assessment(staging, manifest, row)[0], "failed")

    def test_fast_output_exhaustion_is_failure(self):
        with patch.object(evidence, "LIMIT", 128):
            command, stdout, _ = evidence.execute([sys.executable, "-c", 'print("x" * 1000)'], self.root, self.env, self.root, 5)
        self.assertEqual(command["reason"], "output_limit")
        self.assertEqual(len(stdout), 128)

    def test_scoring_requires_local_real_selftest(self):
        stage = self.root / "stage"
        stage.mkdir()
        self.assertFalse(evidence.selftest_ready(stage, [], {"args": ["--toolchain-root", "/toolchain"]}))
        self.assertFalse(evidence.selftest_ready(stage, [], {"args": []}))

    def test_selftest_must_pass_all_cases_and_match_toolchain(self):
        import isolation_selftest
        staging, manifest = self.staged()
        row = manifest["jobs"][0]
        row.update(adapter="isolation-selftest", gate="isolation")
        row["command"]["argv"] = ["fixture", "--toolchain-root", "/toolchain"]
        result = {"status": "passed", "cases": [{"name": name, "passed": True} for name in isolation_selftest.CASES]}
        row["result"] = evidence.blob(staging, evidence.canonical(result))
        self.assertTrue(evidence.selftest_ready(staging, [row], {"args": ["--toolchain-root", "/toolchain"]}))
        self.assertFalse(evidence.selftest_ready(staging, [row], {"args": ["--toolchain-root", "/other"]}))
        result["cases"].pop()
        row["result"] = evidence.blob(staging, evidence.canonical(result))
        self.assertFalse(evidence.selftest_ready(staging, [row], {"args": ["--toolchain-root", "/toolchain"]}))

    def test_committed_fixture_is_verifiable_and_not_ready(self):
        directory = next((evidence.BENCH / "fixtures/release-evidence/v1").glob("*/manifest.json")).parent
        report = evidence.report(directory)
        self.assertEqual(report["purpose"], "fixture")
        self.assertEqual(report["release_status"], "not_ready")
        self.assertNotIn("passed", {g["status"] for g in report["gates"]})


if __name__ == "__main__":
    unittest.main()
