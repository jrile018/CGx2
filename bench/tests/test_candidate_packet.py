import copy
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest


BENCH = Path(__file__).resolve().parents[1]
BASE = BENCH / "fixtures/task-candidates/v3"
sys.path.insert(0, str(BENCH))
import candidate_packet

spec = importlib.util.spec_from_file_location("candidate_accept", BASE / "private/accept.py")
accept = importlib.util.module_from_spec(spec)
spec.loader.exec_module(accept)


class CandidatePacketTests(unittest.TestCase):
    def copy_starter(self, task, language, directory):
        workspace = Path(directory) / "submission"
        shutil.copytree(BASE / "tasks" / task / language, workspace)
        return workspace

    def test_reference_overlay_is_a_valid_submitted_workspace(self):
        for task in accept.SPECS:
            for language in accept.LANGUAGES:
                with self.subTest(task=task, language=language), tempfile.TemporaryDirectory() as directory:
                    workspace = self.copy_starter(task, language, directory)
                    for source in (BASE / "private" / task / language).iterdir():
                        target = workspace / ("src" if language == "cgx2" else "") / source.name
                        shutil.copyfile(source, target)
                    self.assertEqual(accept.validate_workspace(task, language, workspace), workspace)

    def test_starter_wrong_language_symlink_and_private_access_are_rejected(self):
        task = "quote-credit-v3"
        with tempfile.TemporaryDirectory() as directory:
            rust = self.copy_starter(task, "rust", directory)
            with self.assertRaisesRegex(ValueError, "file set"):
                accept.validate_workspace(task, "cgx2", rust)
            with self.assertRaisesRegex(ValueError, "bundled"):
                accept.validate_workspace(task, "rust", BASE / "tasks" / task / "rust")
            link = rust / "private-link"
            link.symlink_to(BASE / "private/accept.py")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                accept.validate_workspace(task, "rust", rust)
            link.unlink()
            source = rust / "adjust.rs"
            source.write_text(source.read_text() + '\nconst LEAK: &str = include_str!("private/accept.py");\n')
            with self.assertRaisesRegex(ValueError, "fixture/file access"):
                accept.validate_workspace(task, "rust", rust)
            source.write_text((BASE / "tasks" / task / "rust/adjust.rs").read_text())
            (rust / "main.rs").write_text((rust / "main.rs").read_text() + "\n")
            with self.assertRaisesRegex(ValueError, "outside editable set"):
                accept.validate_workspace(task, "rust", rust)

    @unittest.skipUnless(shutil.which("rustc"), "rustc unavailable")
    def test_public_example_hardcode_fails_hidden_case(self):
        with tempfile.TemporaryDirectory() as directory:
            rust = self.copy_starter("quote-credit-v3", "rust", directory)
            (rust / "adjust.rs").write_text(
                'pub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64 {\n'
                '    if subtotal == 840 && shipping == 18 && credit == 34 { return 824; }\n'
                '    subtotal.checked_add(shipping).and_then(|n| n.checked_add(credit))'
                '.expect("i64 arithmetic overflow")\n}\n'
            )
            with self.assertRaises(AssertionError):
                accept.check("quote-credit-v3", Path("/unused"),
                             candidate_workspace=rust, language="rust")

    def test_unrun_packet_has_no_performance_claim(self):
        packet = candidate_packet.make_packet()
        self.assertTrue(candidate_packet.validate(packet))
        self.assertEqual({entry["run_state"] for entry in packet["tasks"]}, {"unrun"})
        self.assertEqual({entry["review_state"] for entry in packet["tasks"]}, {"pending"})
        self.assertTrue(all(not entry["trials"] for entry in packet["tasks"]))
        saved = json.loads((BASE / "evidence-unrun.json").read_text())
        self.assertEqual(saved, packet)
        tampered = copy.deepcopy(packet)
        tampered["tasks"][0]["claim_status"] = "win"
        with self.assertRaisesRegex(ValueError, "claim"):
            candidate_packet.validate(tampered)
        tampered = copy.deepcopy(packet)
        tampered["tasks"][0]["harness_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "harness"):
            candidate_packet.validate(tampered)

    def test_external_traces_require_complete_paired_baseline(self):
        common = {
            "task_id": "quote-credit-v3", "model_family": "family-a",
            "model_version": "version-1", "prompt_budget_tokens": 1000,
            "sampling": {"temperature": 0}, "trial_id": "trial-1",
            "workspace_sha256": "a" * 64, "passed": True,
            "input_tokens": 200, "generated_tokens": 100, "total_tokens": 300,
            "latency_ms": 1000, "compile_ms": 100, "run_ms": 10,
        }
        rust = dict(common, language="rust", run_id="rust-1", paired_baseline_run_id=None)
        cgx2 = dict(common, language="cgx2", run_id="cgx2-1",
                    paired_baseline_run_id="rust-1")
        packet = candidate_packet.make_packet(traces=[rust, cgx2], trace_file_sha256="b" * 64)
        self.assertEqual(packet["tasks"][0]["run_state"], "observed")
        self.assertEqual(packet["tasks"][0]["claim_status"], "not_assessed")
        broken = copy.deepcopy(packet)
        broken["tasks"][0]["trials"][1]["paired_baseline_run_id"] = "missing"
        with self.assertRaisesRegex(ValueError, "paired baseline"):
            candidate_packet.validate(broken)
        broken = copy.deepcopy(packet)
        broken["tasks"][0]["trials"][1]["generated_tokens"] = -1
        with self.assertRaisesRegex(ValueError, "generated_tokens"):
            candidate_packet.validate(broken)


if __name__ == "__main__":
    unittest.main()
