import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import readiness
import syntax_packet
import syntax_trace


class SyntaxPacketTests(unittest.TestCase):
    def test_all_six_normalize_to_exact_canonical_source(self):
        result = syntax_packet.validate()
        self.assertEqual(result["case_count"], 6)
        self.assertEqual(result["findings"], [])
        self.assertTrue(all(row["same_semantic_units"] for row in result["cases"]))

    def test_rejects_unbalanced_and_unsupported_braces(self):
        for source in ("fn main() -> i64 {\n    1;\n",
                       "fn main() -> i64 {\n    1 + {2};\n}\n",
                       "fn main() -> i64 {\n    1\n}\n"):
            with self.subTest(source=source), self.assertRaises(ValueError):
                syntax_packet.normalize_brace(source)

    def test_frozen_candidate_hash_detects_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            packet = Path(tmp) / "packet"
            shutil.copytree(syntax_packet.PACKET, packet)
            source = packet / "candidate/brace/precedence.cg"
            source.write_text(source.read_text().replace("84", "85"))
            with self.assertRaisesRegex(ValueError, "frozen SHA-256"):
                syntax_packet.validate(packet)


class SyntaxTraceTests(unittest.TestCase):
    def trace(self, variant="indent", outcome="pass"):
        source = syntax_packet.PACKET / "candidate" / variant / "precedence.cg"
        return {"schema_version": 1, "task_id": "precedence", "variant": variant,
                "model_family": "example-family", "model_version": "example-v1", "seed": 4,
                "usage_provenance": {"kind": "provider_usage", "provider": "example",
                                     "record_id": "external-usage-4"},
                "input_tokens": 50, "output_tokens": 20, "tool_tokens": 4,
                "diagnostic_tokens": 1, "total_tokens": 75,
                "elapsed_wall_seconds": 2.0, "repair_iterations": 0,
                "human_interventions": 0, "source_bytes": source.stat().st_size,
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "verified_outcome": outcome, "verification_ref": "external-check-4",
                "token_budget": 4096, "time_budget_seconds": 120}

    def test_separate_ratios_and_failed_attempt_full_charge(self):
        indent = self.trace()
        brace = self.trace("brace")
        brace["input_tokens"] = 90
        brace["output_tokens"] = 10
        brace["total_tokens"] = 105
        report = syntax_trace.summarize([indent, brace])
        self.assertEqual(report["validated_paired_traces"], 1)
        self.assertEqual(report["genuine_paired_traces"], 0)
        self.assertEqual(report["brace_over_indent_output_token_ratio"], 0.5)
        self.assertEqual(report["brace_over_indent_total_token_ratio"], 105 / 75)
        brace["verified_outcome"] = "fail"
        report = syntax_trace.summarize([indent, brace])
        self.assertIsNone(report["brace_over_indent_total_token_ratio"])
        self.assertEqual(report["all_task_charges"]["brace"]["tokens"], 4096)
        self.assertEqual(report["all_task_charges"]["brace"]["seconds"], 120)

    def test_rejects_missing_provenance_and_inconsistent_totals(self):
        row = self.trace()
        row["usage_provenance"] = {"kind": "provider_usage", "provider": "example"}
        with self.assertRaises(ValueError):
            syntax_trace.validate_trace(row)
        row = self.trace()
        row["total_tokens"] += 1
        with self.assertRaises(ValueError):
            syntax_trace.validate_trace(row)

    def test_rejects_duplicate_and_missing_source_digest(self):
        row = self.trace()
        with self.assertRaises(ValueError):
            syntax_trace.summarize([row, copy.deepcopy(row)])
        row["source_sha256"] = "not-a-sha"
        with self.assertRaises(ValueError):
            syntax_trace.validate_trace(row)

    def test_schema_is_valid_and_accepts_a_trace(self):
        import jsonschema
        schema = json.loads((syntax_packet.ROOT / "schema/syntax-pilot-trace-v1.schema.json").read_text())
        jsonschema.Draft202012Validator.check_schema(schema)
        jsonschema.Draft202012Validator(schema).validate(self.trace())


class ReadinessTests(unittest.TestCase):
    def test_frozen_toolchain_hashes_and_unrun_model_gate(self):
        self.assertEqual(readiness.toolchain_check(syntax_packet.PACKET)["status"], "pass")
        self.assertEqual(syntax_trace.summarize([])["genuine_paired_traces"], 0)


if __name__ == "__main__":
    unittest.main()
