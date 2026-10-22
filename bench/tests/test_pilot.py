import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pilot


class PilotTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.cli = self.root / "cgx2"
        self.cli.write_text(
            f"#!{sys.executable}\n"
            "import sys\n"
            "if sys.argv[1] == 'check':\n"
            "    raise SystemExit(0)\n"
            "print('7')\n",
            encoding="utf-8",
        )
        self.cli.chmod(0o755)
        self.task = {"id": "case", "cgx2": "case.cg", "python": "case.py", "args": [],
                     "oracle": {"stdout": "7\n", "exit_class": "success", "error_class": None},
                     "workload": "one constant"}
        (self.root / "case.cg").write_text("7\n", encoding="utf-8")
        (self.root / "case.py").write_text("print(7)\n", encoding="utf-8")
        self.manifest = self.root / "manifest.json"
        self.manifest.write_text(json.dumps({"schema_version": 1, "tasks": [self.task]}), encoding="utf-8")

    def parity(self):
        return pilot.parity(self.manifest, {"tasks": [self.task]}, self.cli, Path(sys.executable), 2)

    def test_checked_in_manifest_and_pair_coverage(self):
        path, raw, data = pilot.load_manifest()
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(len(data["tasks"]), 6)
        self.assertEqual({x["id"] for x in data["tasks"]},
                         {"precedence", "calls", "mutation", "argument_count", "argument_text", "overflow"})
        self.assertEqual(pilot.sha(raw), pilot.sha(path.read_bytes()))
        for task in data["tasks"]:
            for condition in pilot.CONDITIONS:
                self.assertGreater(pilot.source_size(path.parent / task[condition])["utf8_bytes"], 0)

    def test_manifest_rejects_missing_pair_and_duplicate_id(self):
        data = {"schema_version": 1, "tasks": [dict(self.task) for _ in range(6)]}
        self.manifest.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unique"):
            pilot.load_manifest(self.manifest)
        data["tasks"][1]["id"] = "other"
        self.manifest.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "fixture must match"):
            pilot.load_manifest(self.manifest)

    def test_wrong_output_rejects_parity_and_timing(self):
        (self.root / "case.py").write_text("print(8)\n", encoding="utf-8")
        preflight = self.parity()
        self.assertFalse(preflight["valid"])
        self.assertEqual(preflight["cases"][0]["runs"]["python"]["invalid_reason"], "stdout_mismatch")
        result = pilot.runtime(self.manifest, {"tasks": [self.task]}, self.cli, Path(sys.executable),
                               2, 1, 10, None, preflight)
        self.assertEqual(result["invalid_reason"], "parity_preflight_failed")
        self.assertEqual(result["samples"], [])

    def test_error_class_and_process_failures_are_explicit(self):
        overflow = {"stdout": "", "exit_class": "error", "error_class": "i64_overflow"}
        self.assertIsNone(pilot.assess({"process_error": None, "exit_status": 1,
                                        "stdout": b"", "stderr": b"CGR005 overflow"}, overflow, "cgx2"))
        self.assertEqual(pilot.assess({"process_error": None, "exit_status": 1,
                                       "stdout": b"", "stderr": b"other"}, overflow, "cgx2"),
                         "error_class_mismatch")
        missing = pilot.execute([str(self.root / "missing")], 1)
        self.assertEqual(pilot.assess(missing, self.task["oracle"], "cgx2"), "missing_executable")
        slow = pilot.execute([sys.executable, "-c", "import time; time.sleep(1)"], 0.01)
        self.assertEqual(pilot.assess(slow, self.task["oracle"], "python"), "timeout")

    def test_runtime_alternates_and_excludes_warmups(self):
        preflight = self.parity()
        self.assertTrue(preflight["valid"])
        result = pilot.runtime(self.manifest, {"tasks": [self.task]}, self.cli, Path(sys.executable),
                               2, 2, 10, None, preflight)
        self.assertTrue(result["valid"])
        self.assertEqual(len(result["samples"]), 24)
        pairs = [result["samples"][i:i + 2] for i in range(0, 24, 2)]
        self.assertEqual([pair[0]["condition"] for pair in pairs], ["cgx2", "python"] * 6)
        self.assertEqual([pair[0]["phase"] for pair in pairs], ["warmup"] * 2 + ["timed"] * 10)
        self.assertEqual(result["summary"][0]["conditions"]["cgx2"]["n"], 10)
        self.assertEqual(result["summary"][0]["conditions"]["python"]["n"], 10)
        self.assertTrue(all("stdout_sha256" in sample for sample in result["samples"]))

    def test_failure_during_timing_is_retained_and_invalid(self):
        preflight = self.parity()
        (self.root / "case.py").write_text("print(8)\n", encoding="utf-8")
        result = pilot.runtime(self.manifest, {"tasks": [self.task]}, self.cli, Path(sys.executable),
                               2, 1, 10, None, preflight)
        self.assertFalse(result["valid"])
        self.assertEqual(len(result["samples"]), 22)
        self.assertTrue(any(x["invalid_reason"] == "stdout_mismatch" for x in result["samples"]))
        self.assertTrue(result["summary"][0]["conditions"]["python"]["invalid"])

    def test_output_dir_cannot_overwrite_prior_result(self):
        destination = self.root / "results"
        self.assertEqual(pilot.output_dir(destination), destination)
        (destination / "parity.json").write_text("keep", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            pilot.output_dir(destination)
        self.assertEqual((destination / "parity.json").read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
