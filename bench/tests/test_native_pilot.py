import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_pilot


class NativePilotTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.case = {"id": "one", "cgx2": "one.cg", "rust": "one.rs", "args": [],
                     "oracle": {"stdout": "7\n", "exit_status": 0, "exit_class": "success", "error_class": None}}
        (self.root / "one.cg").write_text("7\n")
        (self.root / "one.rs").write_text('fn main() { println!("7"); }\n')

    def test_parity_failure_blocks_timing(self):
        fake = self.root / "fake"
        fake.write_text("#!/bin/sh\nprintf '8\\n'\n")
        fake.chmod(0o755)
        result = native_pilot.parity([self.case], self.root, {"one": {"cgx2": fake, "rust": fake}}, 2)
        self.assertFalse(result["valid"])
        self.assertEqual(result["cases"][0]["runs"]["cgx2"]["invalid_reason"], "stdout_mismatch")
        self.assertEqual(native_pilot.timing([self.case], self.root, {}, 1, 2, 2, result)["samples"], [])

    def test_exact_exit_status_is_required(self):
        self.case["oracle"] = {"stdout": "", "exit_status": 1, "exit_class": "error",
                               "error_class": "i64_overflow"}
        run = {"process_error": None, "exit_status": 2, "stdout": b"",
               "stderr": b"i64 arithmetic overflow"}
        self.assertEqual(native_pilot.assess(run, self.case["oracle"]), "exit_status_mismatch")

    def test_compiler_failure_and_missing_reference_are_explicit(self):
        bad = self.root / "bad.rs"
        bad.write_text("not rust")
        self.case["rust"] = "bad.rs"
        builds, binaries = native_pilot.build_cases([self.case], self.root, self.root / "out", "/missing/cgx2c", "rustc", 3)
        self.assertFalse(builds["valid"])
        self.assertEqual(builds["cases"][0]["cgx2"]["invalid_reason"], "missing_executable")
        self.assertFalse(builds["cases"][0]["rust"]["valid"])
        self.assertEqual(binaries, {})
        self.case["rust"] = "missing.rs"
        with self.assertRaisesRegex(ValueError, "missing rust reference"):
            native_pilot.validate_cases([self.case], self.root)

    def test_timeout_order_and_all_raw_samples(self):
        slow = self.root / "slow"
        slow.write_text("#!/bin/sh\nsleep 1\n")
        slow.chmod(0o755)
        run = native_pilot.run_binary(slow, [], 0.01)
        self.assertEqual(run["process_error"], "timeout")
        good = self.root / "good"
        good.write_text("#!/bin/sh\nprintf '7\\n'\n")
        good.chmod(0o755)
        binaries = {"one": {"cgx2": good, "rust": good}}
        preflight = native_pilot.parity([self.case], self.root, binaries, 2)
        self.assertTrue(preflight["valid"])
        result = native_pilot.timing([self.case], self.root, binaries, 1, 2, 2, preflight)
        self.assertTrue(result["valid"])
        self.assertEqual(len(result["samples"]), 8)
        self.assertEqual([x["condition"] for x in result["samples"][::2]],
                         ["cgx2", "rust", "cgx2", "rust"])
        self.assertTrue(all("wall_seconds" in x and "pair_index" in x and "phase" in x
                            for x in result["samples"]))
        self.assertEqual(len(result["summary"][0]["conditions"]["cgx2"]["median_bootstrap_95_seconds"]), 2)
        slow_binaries = {"one": {"cgx2": good, "rust": slow}}
        result = native_pilot.timing([self.case], self.root, slow_binaries, 0.01, 1, 1, preflight)
        self.assertFalse(result["valid"])
        self.assertEqual(len(result["samples"]), 4)
        self.assertTrue(any(x["invalid_reason"] == "timeout" for x in result["samples"]))


if __name__ == "__main__":
    unittest.main()
