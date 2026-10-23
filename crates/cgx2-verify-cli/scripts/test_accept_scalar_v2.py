"""Preflight tests only; fixture executables never count as live solver evidence."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

RUNNER = pathlib.Path(__file__).with_name("accept-scalar-v2.py")


class Preflight(unittest.TestCase):
    def test_missing_solver_is_blocked_and_unrun(self):
        with tempfile.TemporaryDirectory() as root:
            output = pathlib.Path(root) / "evidence"
            result = subprocess.run([sys.executable, str(RUNNER), "--solver", str(output / "missing"),
                                     "--verifier", "/missing/verifier", "--output", str(output)],
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 3, result.stderr)
            report = json.loads((output / "acceptance.json").read_text())
            self.assertEqual(report["state"], "blocked/unrun")
            self.assertEqual(report["cases"], [])

    @unittest.skipUnless(os.name == "posix", "executable fixture uses POSIX shell")
    def test_missing_or_wrong_pin_never_executes_solver(self):
        with tempfile.TemporaryDirectory() as root:
            root = pathlib.Path(root)
            marker = root / "executed"
            solver = root / "z3"
            solver.write_text(f"#!/bin/sh\ntouch '{marker}'\n")
            solver.chmod(0o700)
            for name, pin in [("missing-pin", []), ("wrong-pin", ["--sha256", "0" * 64])]:
                output = root / name
                result = subprocess.run([sys.executable, str(RUNNER), "--solver", str(solver),
                                         "--verifier", str(solver), "--output", str(output), *pin],
                                        capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertEqual(json.loads((output / "acceptance.json").read_text())["state"], "blocked/unrun")
                self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
