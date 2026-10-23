import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import syntax_export
import syntax_packet
import syntax_tool
import syntax_trial


class PublicExportTests(unittest.TestCase):
    def test_all_twelve_exports_contain_only_public_material(self):
        packet = syntax_packet.PACKET
        private = [path.read_bytes() for path in (packet / "private").rglob("*") if path.is_file()]
        with tempfile.TemporaryDirectory() as tmp:
            for task in json.loads((packet / "packet.json").read_text())["tasks"]:
                for variant in ("indent", "brace"):
                    with self.subTest(task=task["id"], variant=variant):
                        out = Path(tmp) / f"{task['id']}-{variant}"
                        manifest = syntax_export.export(task["id"], variant, out)
                        self.assertEqual(syntax_export.verify_export(out), manifest)
                        self.assertEqual(set(out.iterdir()), {out / name for name in
                                         (*manifest["files_sha256"], "export-manifest.json")})
                        self.assertNotIn("private", " ".join(path.name for path in out.iterdir()))
                        visible = b"\n".join(path.read_bytes() for path in out.iterdir())
                        self.assertNotIn(b"private/reference", visible)
                        self.assertNotIn(b"private/oracle", visible)
                        for secret in private:
                            self.assertNotIn(secret, visible)
                        starter = (out / "starter.cg").read_text()
                        self.assertIn("TODO()", starter)
                        self.assertEqual(syntax_tool.canonical_source(starter, variant),
                                         syntax_tool.canonical_source(
                                             (packet / task["starter_variants"][variant]).read_text(),
                                             variant))

    def test_rejects_traversal_symlink_and_hash_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            packet = Path(tmp) / "packet"
            shutil.copytree(syntax_packet.PACKET, packet)
            doc = json.loads((packet / "packet.json").read_text())
            doc["tasks"][0]["starter_variants"]["indent"] = "../private/oracle.json"
            (packet / "packet.json").write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "packet frozen SHA-256"):
                syntax_export.export("precedence", "indent", Path(tmp) / "out", packet)
            lock = json.loads((packet / "packet-hashes.json").read_text())
            lock["sha256"]["packet.json"] = hashlib.sha256((packet / "packet.json").read_bytes()).hexdigest()
            (packet / "packet-hashes.json").write_text(json.dumps(lock))
            with self.assertRaisesRegex(ValueError, "starter path escapes"):
                syntax_export.export("precedence", "indent", Path(tmp) / "out", packet)
            shutil.rmtree(packet)
            shutil.copytree(syntax_packet.PACKET, packet)
            starter = packet / "candidate/indent/precedence.cg"
            starter.unlink()
            starter.symlink_to(packet / "private/oracle.json")
            with self.assertRaisesRegex(ValueError, "symlink"):
                syntax_export.export("precedence", "indent", Path(tmp) / "out", packet)
            starter.unlink()
            starter.write_text("TODO()\n")
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                syntax_export.export("precedence", "indent", Path(tmp) / "out", packet)

    def test_detects_export_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            syntax_export.export("precedence", "brace", out)
            (out / "starter.cg").write_text("42\n")
            with self.assertRaisesRegex(ValueError, "export SHA-256 mismatch"):
                syntax_export.verify_export(out)

    def test_rejects_rehashed_public_prompt_with_private_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            packet = Path(tmp) / "packet"
            shutil.copytree(syntax_packet.PACKET, packet)
            doc = json.loads((packet / "packet.json").read_text())
            secret = (packet / "private/reference/indent/precedence.cg").read_text()
            doc["tasks"][0]["expected_behavior"] = secret
            (packet / "packet.json").write_text(json.dumps(doc))
            lock = json.loads((packet / "packet-hashes.json").read_text())
            lock["sha256"]["packet.json"] = hashlib.sha256((packet / "packet.json").read_bytes()).hexdigest()
            (packet / "packet-hashes.json").write_text(json.dumps(lock))
            with self.assertRaisesRegex(ValueError, "private material leaked"):
                syntax_export.export("precedence", "indent", Path(tmp) / "out", packet)


class TrialIntakeTests(unittest.TestCase):
    def trace(self, out):
        names = set(syntax_export.verify_export(out)["files_sha256"]) | {"export-manifest.json"}
        files = {name: (out / name).read_text() for name in names}
        source = "fn main() -> i64:\n    42\n"
        return {
            "schema_version": 2, "task_id": "precedence", "variant": "indent",
            "model_family": "fixture", "model_version": "fixture-1",
            "model_settings": {"temperature": 0, "top_p": 1, "max_output_tokens": 4096},
            "seed": 18, "order_position": 1,
            "export_manifest_sha256": hashlib.sha256((out / "export-manifest.json").read_bytes()).hexdigest(),
            "visible_files": files,
            "transcript": [{"role": "user", "content": "Use the supplied public files."},
                           {"role": "assistant", "content": source}],
            "provider_usage_records": [{"provider": "fixture", "response_id": "fixture-1",
                                        "input_tokens": 100, "output_tokens": 20,
                                        "raw_response": json.dumps({"id": "fixture-1", "usage":
                                            {"input_tokens": 100, "output_tokens": 20}})}],
            "input_tokens": 100, "output_tokens": 20,
            "elapsed_wall_seconds": 1.0, "repair_iterations": 0,
            "human_interventions": 0, "final_source": source,
            "final_source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            "compiler_success": True, "test_success": True, "verification_ref": "fixture-only",
            "isolation": {"status": "pass", "cgroup_cpu": True, "cgroup_memory": True,
                          "cgroup_pids": True, "private_unmounted": True,
                          "network_disabled": True, "grader_separate_process": True},
            "operator_authentication": {"provider_receipts_verified": True,
                                        "isolation_receipt_verified": True,
                                        "grader_ref_verified": True},
            "token_budget": 4096, "time_budget_seconds": 120,
        }

    def test_schema_and_fail_closed_usage_context_isolation(self):
        import jsonschema
        schema = json.loads((Path(__file__).resolve().parents[1] /
                             "schema/syntax-pilot-trial-v2.schema.json").read_text())
        jsonschema.Draft202012Validator.check_schema(schema)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            syntax_export.export("precedence", "indent", out)
            row = self.trace(out)
            jsonschema.Draft202012Validator(schema).validate(row)
            self.assertEqual(syntax_trial.validate_trace(row, out), row)
            missing = copy.deepcopy(row)
            missing["provider_usage_records"] = []
            with self.assertRaisesRegex(ValueError, "usage"):
                syntax_trial.validate_trace(missing, out)
            invented = copy.deepcopy(row)
            invented["output_tokens"] = 21
            with self.assertRaisesRegex(ValueError, "usage totals"):
                syntax_trial.validate_trace(invented, out)
            altered_receipt = copy.deepcopy(row)
            altered_receipt["provider_usage_records"][0]["raw_response"] = json.dumps(
                {"id": "fixture-1", "usage": {"input_tokens": 100, "output_tokens": 21}})
            with self.assertRaisesRegex(ValueError, "raw provider response"):
                syntax_trial.validate_trace(altered_receipt, out)
            leak = copy.deepcopy(row)
            leak["transcript"][0]["content"] = (syntax_packet.PACKET /
                "private/reference/indent/precedence.cg").read_text()
            with self.assertRaisesRegex(ValueError, "private reference"):
                syntax_trial.validate_trace(leak, out)
            isolation = copy.deepcopy(row)
            isolation["isolation"]["cgroup_pids"] = False
            with self.assertRaisesRegex(ValueError, "isolation"):
                syntax_trial.validate_trace(isolation, out)
            tamper = copy.deepcopy(row)
            tamper["visible_files"]["starter.cg"] = "TODO()\n"
            with self.assertRaisesRegex(ValueError, "visible file differs"):
                syntax_trial.validate_trace(tamper, out)

    def test_unregistered_model_cannot_be_scored(self):
        with self.assertRaisesRegex(ValueError, "not frozen"):
            syntax_trial.paired_report([])

    def test_complete_balanced_grid_charges_failures_and_bootstraps_tasks(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            syntax_export.export("precedence", "indent", out)
            template = self.trace(out)
            protocol = json.loads(syntax_trial.PROTOCOL.read_text())
            protocol["status"] = "frozen"
            protocol["registered_model"] = {key: template[key] for key in
                                            ("model_family", "model_version", "model_settings", "seed")}
            rows = []
            for item in protocol["order"]:
                for position, variant in enumerate((item["first"], item["second"]), 1):
                    row = copy.deepcopy(template)
                    row["task_id"] = item["task_id"]
                    row["variant"] = variant
                    row["order_position"] = position
                    row["provider_usage_records"][0]["response_id"] = f"{item['task_id']}-{variant}"
                    if item["task_id"] == "overflow" and variant == "indent":
                        row["test_success"] = False
                    rows.append(row)
            report = syntax_trial.paired_report(rows, protocol)
            self.assertEqual(report["paired_successes"], 5)
            self.assertEqual(report["completion_difference_indent_minus_brace"], -1 / 6)
            self.assertEqual(report["all_task_charges"]["indent"]["tokens"], 5 * 120 + 4096)
            self.assertEqual(report["all_task_charges"]["indent"]["seconds"], 5 + 120)
            self.assertEqual(len(report["completion_difference_95pct_task_bootstrap"]), 2)
            altered = copy.deepcopy(rows)
            altered[0]["model_settings"]["temperature"] = 0.5
            with self.assertRaisesRegex(ValueError, "model, settings"):
                syntax_trial.paired_report(altered, protocol)


if __name__ == "__main__":
    unittest.main()
