import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import runner


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.case = {
            "task_id": "smoke", "tier": "small", "language": "python",
            "variant": "standard", "model_id": "fixture", "argv": [sys.executable, "-c", "print('ok')"],
            "cwd": ".", "timeout_seconds": 1, "seed": 4, "expected_exit": 0,
            "hidden_test_argv": [sys.executable, "-c", "print('private marker')"],
            "budget": {"total_tokens": 1000}, "versions": {"python": "3.14.4"},
            "environment": {"BENCH_FLAG": "public"},
        }

    def run_case(self):
        path = self.root / "task.json"
        path.write_text(json.dumps(self.case))
        return runner.run_manifest(path)

    def test_success_hashes_output_without_leaking_hidden_test(self):
        result = self.run_case()
        self.assertTrue(result["verified_completion"])
        self.assertEqual(result["exit_status"], 0)
        self.assertEqual(len(result["stdout_sha256"]), 64)
        self.assertEqual(len(result["hidden_test_stdout_sha256"]), 64)
        self.assertNotIn("private marker", json.dumps(result))
        self.assertIsNone(result["total_tokens"])
        self.assertIsNone(result["output_tokens"])

    def test_nonzero_exit_is_failure(self):
        self.case["argv"] = [sys.executable, "-c", "raise SystemExit(3)"]
        result = self.run_case()
        self.assertFalse(result["verified_completion"])
        self.assertEqual(result["exit_status"], 3)
        self.assertEqual(result["failure_reason"], "unexpected_exit")

    def test_timeout_uses_full_budget_and_terminates_child(self):
        self.case["timeout_seconds"] = 0.05
        self.case["argv"] = [sys.executable, "-c", "import time\ntime.sleep(2)"]
        result = self.run_case()
        self.assertEqual(result["failure_reason"], "timeout")
        self.assertEqual(result["charged_wall_seconds"], 0.05)
        self.assertFalse(result["verified_completion"])

    def test_missing_executable_is_recorded(self):
        self.case["argv"] = ["/nonexistent/cgx2-bench-executable"]
        result = self.run_case()
        self.assertEqual(result["failure_reason"], "missing_executable")
        self.assertFalse(result["verified_completion"])

    def test_shell_like_or_malformed_argv_is_rejected(self):
        for argv in ("echo pwned", [], ["echo", "ok; touch /tmp/nope"], ["echo", "$(whoami)"], ["echo", 7], ["sh", "-c", "echo safe-looking"]):
            with self.subTest(argv=argv):
                self.case["argv"] = argv
                with self.assertRaises(ValueError):
                    self.run_case()

    def test_hidden_test_failure_fails_completion(self):
        self.case["hidden_test_argv"] = [sys.executable, "-c", "raise SystemExit(2)"]
        result = self.run_case()
        self.assertEqual(result["failure_reason"], "hidden_test_failed")
        self.assertFalse(result["verified_completion"])

    def test_provenance_and_trace_are_captured_without_secret_environment(self):
        trace = self.root / "trace.json"
        trace.write_text(json.dumps({"output_tokens": 12, "total_tokens": 50, "iterations": 2, "human_interventions": 1}))
        self.case["trace_path"] = "trace.json"
        self.case["environment"]["PASSWORD"] = "do-not-report"
        self.case["environment"]["AWS_ACCESS_KEY_ID"] = "top-secret-key-value"
        self.case["versions"]["API_TOKEN"] = "hidden-version-value"
        result = self.run_case()
        self.assertEqual(result["total_tokens"], 50)
        self.assertEqual(result["iterations"], 2)
        self.assertEqual(result["provenance"]["versions"], {"python": "3.14.4"})
        self.assertNotIn("do-not-report", json.dumps(result))
        self.assertNotIn("top-secret-key-value", json.dumps(result))
        self.assertNotIn("hidden-version-value", json.dumps(result))

    def test_unlisted_host_secret_is_not_passed_to_case(self):
        key = "CGX2_TEST_HOST_SECRET"
        previous = os.environ.get(key)
        os.environ[key] = "secret"
        self.addCleanup(lambda: os.environ.pop(key, None) if previous is None else os.environ.__setitem__(key, previous))
        self.case["argv"] = [sys.executable, "-c", f"import os\nraise SystemExit(1 if os.getenv('{key}') else 0)"]
        result = self.run_case()
        self.assertTrue(result["verified_completion"])


if __name__ == "__main__":
    unittest.main()
