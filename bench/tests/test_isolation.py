"""Fail-closed contract for the scored worker and its adversarial fixtures."""

import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock


BENCH = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH))
import isolation  # noqa: E402
import isolation_selftest  # noqa: E402
import scored_candidate  # noqa: E402


class IsolationTests(unittest.TestCase):
    class TrustedScope:
        """Unit-test capture plumbing without claiming host containment."""

        def __enter__(self):
            self.exited = False
            return self

        def __exit__(self, *_args):
            self.exited = True

        def join_self(self):
            pass

        def cpu_usage_us(self):
            return 0

    def test_missing_aggregate_controllers_prevent_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            (parent / "cgroup.subtree_control").write_text("cpu memory")
            with mock.patch.object(isolation, "_cgroup_parent", return_value=parent), \
                 mock.patch.object(isolation.subprocess, "Popen", side_effect=AssertionError("launched")):
                with self.assertRaisesRegex(isolation.UnsupportedIsolation, "pids"):
                    isolation.capture(["/bin/true"], 3)

    def test_cgroup_scope_configures_and_reaps_all_aggregate_controls(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            (parent / "cgroup.subtree_control").write_text("cpu memory pids")
            mkdir = Path.mkdir
            rmdir = Path.rmdir

            def fake_mkdir(path, *args, **kwargs):
                result = mkdir(path, *args, **kwargs)
                if path.parent == parent and path.name.startswith("cgx2-bench-"):
                    for name, value in {"cgroup.procs": "", "cgroup.events": "populated 0\n",
                                        "cgroup.kill": "", "cpu.stat": "usage_usec 0\n"}.items():
                        (path / name).write_text(value)
                return result

            def fake_rmdir(path):
                if path.parent == parent and path.name.startswith("cgx2-bench-"):
                    for child in path.iterdir():
                        child.unlink()
                return rmdir(path)

            with mock.patch.object(isolation, "_cgroup_parent", return_value=parent), \
                 mock.patch.object(Path, "mkdir", fake_mkdir), \
                 mock.patch.object(Path, "rmdir", fake_rmdir):
                with isolation.CgroupScope() as scope:
                    path = scope.path
                    self.assertEqual((path / "cpu.max").read_text(), "100000 100000")
                    self.assertEqual((path / "memory.max").read_text(), str(isolation.LIMITS["address_bytes"]))
                    self.assertEqual((path / "pids.max").read_text(), str(isolation.LIMITS["processes"]))
                    self.assertEqual((path / "memory.oom.group").read_text(), "1")
            self.assertFalse(path.exists())

    def test_unsupported_capability_never_launches_candidate(self):
        evidence = {"status": "unrun", "cases": []}
        with mock.patch.object(isolation, "probe",
                               side_effect=isolation.UnsupportedIsolation("user_namespace_unavailable")), \
             mock.patch.object(isolation, "capture", side_effect=AssertionError("launch reached")):
            result = scored_candidate._score_snapshot(evidence, {}, "task", "rust", Path("/missing"))
        self.assertEqual(result["status"], "unsupported_isolation")
        self.assertEqual(result["reason"], "user_namespace_unavailable")
        self.assertEqual(result["cases"], [])

    def test_compile_failure_is_not_reported_as_isolation_failure(self):
        passed, reason = scored_candidate._evaluate(
            {"reason": None, "exit_code": 100, "stdout": b"",
             "stderr": b"build_failed\ncompiler diagnostic\n"}, 7)
        self.assertFalse(passed)
        self.assertEqual(reason, "build_failed")

    def test_unreviewed_candidate_never_reaches_probe_or_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            toolchain = Path(directory) / "toolchain"
            (toolchain / "bin").mkdir(parents=True)
            (toolchain / "bin/compiler").write_bytes(b"trusted-test-compiler")
            source = scored_candidate.DEFAULT_PACK / "tasks/quote-credit-v3/rust"
            candidate = Path(directory) / "candidate"
            shutil.copytree(source, candidate)
            with mock.patch.object(isolation, "probe", side_effect=AssertionError("probe reached")), \
                 mock.patch.object(isolation, "capture", side_effect=AssertionError("launch reached")):
                result = scored_candidate.score(scored_candidate.DEFAULT_PACK,
                                                "quote-credit-v3", "rust", candidate,
                                                toolchain, "bin/compiler")
            self.assertEqual(result["status"], "unrun")
            self.assertIn("independent frozen approval", result["reason"])
            self.assertEqual(result["cases"], [])

    def test_source_regex_is_not_the_isolation_boundary(self):
        base = scored_candidate.DEFAULT_PACK
        pack, cards = scored_candidate.candidate_packet.read_material(base)
        task = next(task for task in pack["tasks"] if task["task_id"] == "quote-credit-v3")
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "candidate"
            shutil.copytree(base / task["workspace_paths"]["rust"], candidate)
            allowed = next(Path(name).relative_to("rust") for name in cards[task["task_id"]]["allowed_files"]
                           if name.startswith("rust/") and name.endswith(".rs"))
            with (candidate / allowed).open("a", encoding="utf-8") as handle:
                handle.write("\n// std::fs and std::process are judged by the sandbox\n")
            self.assertEqual(scored_candidate._check_workspace(base, task, cards[task["task_id"]],
                                                               "rust", candidate), candidate)
            (candidate / allowed).unlink()
            (candidate / allowed).symlink_to(base / "private/accept.py")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                scored_candidate._check_workspace(base, task, cards[task["task_id"]],
                                                  "rust", candidate)

    def test_profile_has_no_writable_host_mount(self):
        with tempfile.TemporaryDirectory() as directory:
            fd = os.memfd_create("test-filter")
            try:
                argv = isolation.profile_argv(directory, directory, fd, ["/usr/bin/true"])
            finally:
                os.close(fd)
        self.assertIn("--unshare-net", argv)
        self.assertIn("--unshare-user", argv)
        self.assertIn("--seccomp", argv)
        self.assertNotIn("--bind", argv)
        self.assertIn("--tmpfs", argv)
        self.assertIn("--cap-drop", argv)
        self.assertIn("--disable-userns", argv)
        with self.assertRaisesRegex(isolation.UnsupportedIsolation, "host_home"):
            isolation.profile_argv(Path.home(), Path.home(), 3, ["/usr/bin/true"])

    def test_output_limit_kills_process_group(self):
        fd = os.memfd_create("test-filter")
        try:
            with mock.patch.object(isolation, "CgroupScope", self.TrustedScope):
                observed = isolation.capture([sys.executable, "-c",
                                              "import sys; sys.stdout.write('x'*1200000)"], fd)
        finally:
            os.close(fd)
        self.assertEqual(observed["reason"], "output_limit")
        self.assertLessEqual(len(observed["stdout"]), isolation.LIMITS["output_bytes"])

    def test_inherited_private_descriptor_is_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            private = Path(directory) / "private-oracle"
            private.write_text("secret")
            fd = os.open(private, os.O_RDONLY)
            seccomp_fd = os.memfd_create("test-filter")
            try:
                code = ("import os; print('escaped' if any("
                        "os.readlink('/proc/self/fd/'+str(n)) == " + repr(str(private)) +
                        " for n in range(3,64) if os.path.exists('/proc/self/fd/'+str(n))) "
                        "else 'denied')")
                with mock.patch.object(isolation, "CgroupScope", self.TrustedScope):
                    observed = isolation.capture([sys.executable, "-c", code], seccomp_fd)
            finally:
                os.close(fd)
                os.close(seccomp_fd)
        self.assertEqual(observed["stdout"], b"denied\n")

    def test_timeout_reaps_scope(self):
        scope = self.TrustedScope()
        fd = os.memfd_create("test-filter")
        try:
            with mock.patch.object(isolation, "CgroupScope", return_value=scope), \
                 mock.patch.dict(isolation.LIMITS, {"wall_seconds": 0.05}):
                observed = isolation.capture([sys.executable, "-c", "import time; time.sleep(5)"], fd)
        finally:
            os.close(fd)
        self.assertEqual(observed["reason"], "wall_timeout")
        self.assertTrue(scope.exited)

    def test_aggregate_cpu_budget_reaps_scope(self):
        scope = self.TrustedScope()
        scope.cpu_usage_us = lambda: isolation.LIMITS["aggregate_cpu_seconds"] * 1_000_000
        fd = os.memfd_create("test-filter")
        try:
            with mock.patch.object(isolation, "CgroupScope", return_value=scope):
                observed = isolation.capture([sys.executable, "-c", "import time; time.sleep(5)"], fd)
        finally:
            os.close(fd)
        self.assertEqual(observed["reason"], "aggregate_cpu_limit")
        self.assertTrue(scope.exited)

    def test_adversarial_fixture_inventory(self):
        self.assertEqual(set(isolation_selftest.CASES),
                         {path.stem for path in isolation_selftest.FIXTURES.glob("*.rs")})
        self.assertIn("openat", (isolation_selftest.FIXTURES / "alternate_file_api.rs").read_text())
        self.assertIn("symlink", (isolation_selftest.FIXTURES / "symlink_traversal.rs").read_text())
        self.assertIn("Command::new", (isolation_selftest.FIXTURES / "child_process_burst.rs").read_text())
        self.assertIn("/proc/self/fd", (isolation_selftest.FIXTURES / "descriptor_inheritance.rs").read_text())


if __name__ == "__main__":
    unittest.main()
