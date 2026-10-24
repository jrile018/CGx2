#!/usr/bin/env python3
"""Safe finite synthetic children; no Cargo, disk output capture or host changes."""
import base64
import importlib.util
import json
import os
from pathlib import Path
import resource
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from supervisor import evidence, supervise

HERE = Path(__file__).resolve().parent
CAP = 16384


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (128 * 1024**2, 128 * 1024**2))
    resource.setrlimit(resource.RLIMIT_CPU, (2, 2))
    resource.setrlimit(resource.RLIMIT_FSIZE, (65536, 65536))


def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE.parent / "owned-result-evidence" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SupervisorTests(unittest.TestCase):
    def run_child(self, source, *, merged=False, timeout=2):
        result = supervise([sys.executable, "-c", source], cwd=HERE, env=os.environ,
                           limits=limits, cap=CAP, timeout=timeout, merged=merged)
        self.assertLessEqual(len(result["stdout"]), CAP)
        self.assertLessEqual(len(result["stderr"]), CAP)
        self.assertLess(result["duration_seconds"], 3)
        return result

    def test_success_and_exact_cap(self):
        for merged in (False, True):
            with self.subTest(merged=merged):
                r = self.run_child(f"import os; os.write(1,b'x'*{CAP})", merged=merged)
                self.assertEqual((r["reason"], r["exit"], r["stdout"]), ("exit", 0, b"x" * CAP))

    def test_nonzero_and_diagnostics(self):
        for merged in (False, True):
            r = self.run_child("import os; os.write(2,b'CGR080 actual diagnostic\\n'); raise SystemExit(7)", merged=merged)
            self.assertEqual((r["reason"], r["exit"]), ("exit", 7))
            self.assertIn(b"CGR080 actual diagnostic", r["stdout"] + r["stderr"])

    def test_overflow_stdout_stderr_and_simultaneous(self):
        # Each child attempts at most 128 KiB, even if supervision regresses.
        for merged in (False, True):
            for streams in ((1,), (2,), (1, 2)):
                with self.subTest(merged=merged, streams=streams):
                    source = ("import os, threading\n"
                              f"threads=[threading.Thread(target=os.write,args=(fd,b'x'*65536)) for fd in {streams!r}]\n"
                              "[t.start() for t in threads]\n[t.join() for t in threads]")
                    r = self.run_child(source, merged=merged)
                    self.assertEqual(r["reason"], "output_overflow")
                    self.assertEqual(r["signal"], signal.SIGKILL)
                    self.assertEqual(len(r[r["overflow_stream"]]), CAP)

    def test_timeout(self):
        for merged in (False, True):
            r = self.run_child("import time; time.sleep(2)", timeout=0.15, merged=merged)
            self.assertEqual((r["reason"], r["exit"]), ("timeout", -signal.SIGKILL))

    def test_signal_is_not_expected_error(self):
        r = self.run_child("import os,signal; os.kill(os.getpid(),signal.SIGTERM)")
        self.assertEqual((r["reason"], r["exit"], r["signal"]), ("signal", -15, 15))

    def test_non_utf8(self):
        r = self.run_child("import os; os.write(1,b'\\xffok'); os.write(2,b'\\xfeerror')")
        record = evidence(r)
        self.assertEqual(record["stdout"], "\ufffdok")
        self.assertEqual(base64.b64decode(record["stderr_base64"]), b"\xfeerror")
        self.assertEqual(r["reason"], "exit")

    def test_missing_executable(self):
        r = supervise([str(HERE / "does-not-exist")], cwd=HERE, env=os.environ,
                      limits=limits, cap=CAP, timeout=1)
        self.assertEqual((r["reason"], r["exit"]), ("spawn_error", None))
        self.assertIn("does-not-exist", r["error"])

    def test_descendants_on_exit_timeout_and_overflow(self):
        # One fork only, independently self-terminating in two seconds.
        for merged in (False, True):
            for mode in ("exit", "timeout", "output_overflow"):
                with self.subTest(merged=merged, mode=mode):
                    tail = {"exit": "os._exit(0)", "timeout": "time.sleep(2)",
                            "output_overflow": "os.write(2,b'x'*65536); time.sleep(2)"}[mode]
                    source = ("import os,time\npid=os.fork()\n"
                              "if pid==0:\n time.sleep(2)\n os._exit(0)\n"
                              "os.write(1,(str(pid)+'\\n').encode())\n" + tail)
                    r = self.run_child(source, merged=merged, timeout=0.25)
                    self.assertEqual(r["reason"], mode)
                    pid = int(r["stdout"].splitlines()[0])
                    # Orphan zombies await host init; they cannot execute/retain pipes.
                    state = Path(f"/proc/{pid}/stat")
                    deadline = time.monotonic() + 0.3
                    while state.exists() and state.read_text().split()[2] != "Z" and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(not state.exists() or state.read_text().split()[2] == "Z")


class ReportingTests(unittest.TestCase):
    def test_inner_failure_is_saved_before_assertion(self):
        module = load("run")
        for reason, rc in (("output_overflow", -9), ("timeout", -9), ("signal", -15), ("exit", 7), ("spawn_error", None)):
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as tmp:
                output = Path(tmp) / "product.json"
                result = {"exit": rc, "reason": reason, "stdout": b"\xff", "stderr": b"actual diagnostic"}
                with patch.object(module, "supervise", return_value=result), patch.dict(os.environ, TMPDIR=tmp), patch.object(sys, "argv", ["run.py", "--binary-dir", tmp, "--output", str(output)]):
                    with self.assertRaises(AssertionError):
                        module.main()
                report = json.loads(output.read_text())
                record = report["commands"][0]
                self.assertEqual((record["exit"], record["reason"], record["stderr"]), (rc, reason, "actual diagnostic"))
                self.assertTrue(record["command"])
                self.assertEqual(report["failed"], 1)
                self.assertEqual(base64.b64decode(record["stdout_base64"]), b"\xff")

    def test_outer_failure_keeps_raw_log_and_reason(self):
        module = load("verify")
        for reason in ("output_overflow", "timeout", "signal", "spawn_error"):
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as tmp:
                result = {"exit": None if reason == "spawn_error" else -9, "reason": reason,
                          "stdout": b"\xffactual diagnostic", "stderr": b""}
                with patch.object(module, "supervise", return_value=result), patch.dict(os.environ, TMPDIR=tmp, CARGO_TARGET_DIR=tmp), patch.object(sys, "argv", ["verify.py", "--output-dir", tmp]):
                    self.assertEqual(module.main(), 1)
                report = json.loads((Path(tmp) / "verification.json").read_text())
                self.assertTrue(report["checks"])
                for record in report["checks"]:
                    self.assertEqual((record["reason"], record["exit"]), (reason, result["exit"]))
                    self.assertTrue(record["command"])
                    self.assertEqual((Path(tmp) / record["log"]).read_bytes(), result["stdout"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
