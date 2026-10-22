"""Linux Bubblewrap profile for benchmark submissions.

The host sees the oracle.  The worker sees only a copied source tree, a
read-only toolchain, system libraries, and private tmpfs scratch space.
"""

import ctypes
import ctypes.util
import errno
import hashlib
import os
from pathlib import Path
import platform
import resource
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid


PROFILE_VERSION = 2
LIMITS = {
    "cpu_seconds": 90,
    "wall_seconds": 120,
    "address_bytes": 2 * 1024**3,
    "file_bytes": 64 * 1024**2,
    "output_bytes": 1_000_000,
    "open_files": 64,
    "processes": 32,
    "aggregate_cpu_seconds": 90,
    "aggregate_cpu_quota_us": 100_000,
    "aggregate_cpu_period_us": 100_000,
    "scratch_bytes": 256 * 1024**2,
    "tmp_bytes": 16 * 1024**2,
}
NETWORK_SYSCALLS = (
    "socket", "socketpair", "connect", "bind", "listen", "accept", "accept4",
    "sendto", "sendmsg", "recvfrom", "recvmsg",
)
PRIVILEGED_SYSCALLS = (
    "mount", "umount2", "pivot_root", "ptrace", "bpf", "userfaultfd",
    "keyctl", "add_key", "request_key", "open_by_handle_at", "init_module",
    "finit_module", "delete_module", "kexec_load", "reboot", "swapon",
    "swapoff", "unshare", "setns",
)
PROCESS_SYSCALLS = ("clone", "clone3", "fork", "vfork")
LANDLOCK_NUMBERS = {"x86_64": (444, 445, 446), "aarch64": (444, 445, 446)}
LANDLOCK_ACCESS_FS_EXECUTE = 1
LANDLOCK_RULE_PATH_BENEATH = 1
LANDLOCK_CREATE_RULESET_VERSION = 1
PR_SET_NO_NEW_PRIVS = 38


class UnsupportedIsolation(RuntimeError):
    """A required OS isolation feature is unavailable."""


def file_sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def tree_sha256(root):
    """Hash paths and bytes in the copied, symlink-free candidate snapshot."""
    hasher = hashlib.sha256()
    for path in sorted(Path(root).rglob("*")):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("candidate snapshot contains symlink or special file")
        if path.is_file():
            rel = path.relative_to(root).as_posix().encode()
            data = path.read_bytes()
            hasher.update(len(rel).to_bytes(8, "big") + rel)
            hasher.update(len(data).to_bytes(8, "big") + data)
    return hasher.hexdigest()


def toolchain_sha256(root):
    """Attest the complete read-only toolchain mount, including link targets."""
    root = Path(root)
    hasher = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix().encode()
        hasher.update(len(relative).to_bytes(8, "big") + relative)
        if path.is_symlink():
            target = os.readlink(path).encode()
            hasher.update(b"L" + len(target).to_bytes(8, "big") + target)
        elif path.is_dir():
            hasher.update(b"D")
        elif path.is_file():
            hasher.update(b"F")
            with path.open("rb") as handle:
                for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                    hasher.update(chunk)
        else:
            raise ValueError(f"toolchain contains special file: {relative!r}")
    return hasher.hexdigest()


def _seccomp_library():
    name = ctypes.util.find_library("seccomp") or "libseccomp.so.2"
    try:
        lib = ctypes.CDLL(name, use_errno=True)
    except OSError as error:
        raise UnsupportedIsolation(f"libseccomp_missing:{error}") from error
    lib.seccomp_init.argtypes = [ctypes.c_uint32]
    lib.seccomp_init.restype = ctypes.c_void_p
    lib.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]
    lib.seccomp_syscall_resolve_name.restype = ctypes.c_int
    lib.seccomp_rule_add.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint]
    lib.seccomp_rule_add.restype = ctypes.c_int
    lib.seccomp_export_bpf.argtypes = [ctypes.c_void_p, ctypes.c_int]
    lib.seccomp_export_bpf.restype = ctypes.c_int
    lib.seccomp_load.argtypes = [ctypes.c_void_p]
    lib.seccomp_load.restype = ctypes.c_int
    lib.seccomp_release.argtypes = [ctypes.c_void_p]
    return lib


def _seccomp_context(runtime=False):
    lib = _seccomp_library()
    ctx = lib.seccomp_init(0x7FFF0000)  # SCMP_ACT_ALLOW
    if not ctx:
        raise UnsupportedIsolation("seccomp_init_failed")
    try:
        names = NETWORK_SYSCALLS + PRIVILEGED_SYSCALLS
        if runtime:
            names += PROCESS_SYSCALLS
        for name in names:
            number = lib.seccomp_syscall_resolve_name(name.encode())
            if number < 0:
                raise UnsupportedIsolation(f"seccomp_syscall_unknown:{name}")
            result = lib.seccomp_rule_add(ctx, 0x00050000 | errno.EPERM, number, 0)
            if result:
                raise UnsupportedIsolation(f"seccomp_rule_failed:{name}:{-result}")
    except BaseException:
        lib.seccomp_release(ctx)
        raise
    return lib, ctx


def export_seccomp_fd():
    lib, ctx = _seccomp_context()
    try:
        fd = os.memfd_create("cgx2-benchmark-seccomp", 0)
        if lib.seccomp_export_bpf(ctx, fd):
            os.close(fd)
            raise UnsupportedIsolation("seccomp_export_failed")
        os.lseek(fd, 0, os.SEEK_SET)
        return fd
    finally:
        lib.seccomp_release(ctx)


def _no_new_privileges():
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0):
        raise UnsupportedIsolation(f"no_new_privileges_failed:{ctypes.get_errno()}")


def install_runtime_seccomp():
    _no_new_privileges()
    lib, ctx = _seccomp_context(runtime=True)
    try:
        if lib.seccomp_load(ctx):
            raise UnsupportedIsolation(f"runtime_seccomp_load_failed:{ctypes.get_errno()}")
    finally:
        lib.seccomp_release(ctx)


class _RulesetAttr(ctypes.Structure):
    _fields_ = [("handled_access_fs", ctypes.c_uint64)]


class _PathBeneathAttr(ctypes.Structure):
    _fields_ = [("allowed_access", ctypes.c_uint64), ("parent_fd", ctypes.c_int32)]


def _landlock_syscalls():
    numbers = LANDLOCK_NUMBERS.get(platform.machine())
    if not numbers:
        raise UnsupportedIsolation(f"landlock_arch_unsupported:{platform.machine()}")
    return ctypes.CDLL(None, use_errno=True).syscall, numbers


def landlock_abi():
    syscall, (create, _, _) = _landlock_syscalls()
    abi = syscall(create, 0, 0, LANDLOCK_CREATE_RULESET_VERSION)
    if abi < 1:
        raise UnsupportedIsolation(f"landlock_unavailable:{ctypes.get_errno()}")
    return abi


def install_exec_landlock(executable):
    """Allow execution of the result and its loader libraries only."""
    landlock_abi()
    _no_new_privileges()
    syscall, (create, add, restrict) = _landlock_syscalls()
    rules = _RulesetAttr(LANDLOCK_ACCESS_FS_EXECUTE)
    rules_fd = syscall(create, ctypes.byref(rules), ctypes.sizeof(rules), 0)
    if rules_fd < 0:
        raise UnsupportedIsolation(f"landlock_ruleset_failed:{ctypes.get_errno()}")
    try:
        # A dynamic ELF may need execute permission on its interpreter.  Do
        # not grant directory-wide execution: that would permit host tools.
        for path in (str(executable), "/lib64/ld-linux-x86-64.so.2",
                     "/lib/ld-linux-aarch64.so.1"):
            if not Path(path).exists():
                continue
            parent_fd = os.open(path, os.O_PATH | os.O_CLOEXEC)
            try:
                rule = _PathBeneathAttr(LANDLOCK_ACCESS_FS_EXECUTE, parent_fd)
                if syscall(add, rules_fd, LANDLOCK_RULE_PATH_BENEATH,
                           ctypes.byref(rule), 0):
                    raise UnsupportedIsolation(f"landlock_rule_failed:{path}:{ctypes.get_errno()}")
            finally:
                os.close(parent_fd)
        if syscall(restrict, rules_fd, 0):
            raise UnsupportedIsolation(f"landlock_restrict_failed:{ctypes.get_errno()}")
    finally:
        os.close(rules_fd)


def profile_argv(workspace, toolchain, seccomp_fd, command):
    bwrap = shutil.which("bwrap")
    if not bwrap:
        raise UnsupportedIsolation("bubblewrap_missing")
    workspace = Path(workspace).resolve(strict=True)
    toolchain = Path(toolchain).resolve(strict=True)
    if (Path.home().is_relative_to(toolchain) or
            Path(__file__).resolve().parent.is_relative_to(toolchain) or
            Path(__file__).resolve().parent.is_relative_to(workspace)):
        raise UnsupportedIsolation("mount_would_expose_host_home_or_repository")
    args = [bwrap, "--unshare-user", "--unshare-pid", "--unshare-net",
            "--unshare-ipc", "--unshare-uts", "--die-with-parent", "--new-session",
            "--disable-userns", "--assert-userns-disabled", "--uid", "65534",
            "--gid", "65534", "--cap-drop", "ALL", "--clearenv",
            "--setenv", "PATH", "/toolchain/bin:/usr/bin:/bin",
            "--setenv", "HOME", "/scratch", "--setenv", "TMPDIR", "/scratch",
            "--setenv", "LANG", "C.UTF-8", "--setenv", "LC_ALL", "C.UTF-8",
            "--setenv", "RUSTC_BOOTSTRAP", "0"]
    for system_path in ("/usr", "/bin", "/lib", "/lib64",
                        "/etc/alternatives", "/etc/ld.so.cache"):
        if Path(system_path).exists():
            args += ["--ro-bind", system_path, system_path]
    args += ["--ro-bind", str(toolchain), "/toolchain",
             "--ro-bind", str(workspace), "/workspace",
             "--size", str(LIMITS["scratch_bytes"]), "--tmpfs", "/scratch",
             "--size", str(LIMITS["tmp_bytes"]), "--tmpfs", "/tmp",
             "--proc", "/proc", "--dev", "/dev", "--chdir", "/workspace",
             "--seccomp", str(seccomp_fd), "--"]
    return args + command


def _limits():
    resource.setrlimit(resource.RLIMIT_CPU, (LIMITS["cpu_seconds"], LIMITS["cpu_seconds"]))
    resource.setrlimit(resource.RLIMIT_AS, (LIMITS["address_bytes"], LIMITS["address_bytes"]))
    resource.setrlimit(resource.RLIMIT_FSIZE, (LIMITS["file_bytes"], LIMITS["file_bytes"]))
    resource.setrlimit(resource.RLIMIT_NOFILE, (LIMITS["open_files"], LIMITS["open_files"]))
    resource.setrlimit(resource.RLIMIT_NPROC, (LIMITS["processes"], LIMITS["processes"]))
    os.umask(0o077)


def _cgroup_parent():
    """Find this process's cgroup v2 directory without trusting an environment path."""
    mount = Path("/sys/fs/cgroup").resolve()
    if not (mount / "cgroup.controllers").is_file():
        raise UnsupportedIsolation("cgroup_v2_unavailable")
    entries = [line.split("::", 1)[1] for line in Path("/proc/self/cgroup").read_text().splitlines()
               if line.startswith("0::")]
    if len(entries) != 1:
        raise UnsupportedIsolation("cgroup_v2_membership_unavailable")
    parent = (mount / entries[0].lstrip("/")).resolve()
    if not parent.is_relative_to(mount) or not parent.is_dir():
        raise UnsupportedIsolation("cgroup_v2_parent_invalid")
    return parent


class CgroupScope:
    """One aggregate resource boundary for a worker and every descendant."""

    def __init__(self):
        self.path = None

    def __enter__(self):
        try:
            parent = _cgroup_parent()
            enabled = set((parent / "cgroup.subtree_control").read_text().split())
        except OSError as error:
            raise UnsupportedIsolation(f"cgroup_probe_failed:{error}") from error
        required = {"cpu", "memory", "pids"}
        if not required <= enabled:
            raise UnsupportedIsolation("cgroup_controllers_not_delegated:" +
                                       ",".join(sorted(required - enabled)))
        path = parent / f"cgx2-bench-{os.getpid()}-{uuid.uuid4().hex}"
        try:
            path.mkdir(mode=0o700)
            self.path = path
            settings = {
                "cpu.max": f"{LIMITS['aggregate_cpu_quota_us']} {LIMITS['aggregate_cpu_period_us']}",
                "memory.max": str(LIMITS["address_bytes"]),
                "pids.max": str(LIMITS["processes"]),
                "memory.oom.group": "1",
            }
            for name, value in settings.items():
                target = path / name
                target.write_text(value)
                if target.read_text().strip() != value:
                    raise UnsupportedIsolation(f"cgroup_limit_unverified:{name}")
            for name in ("cgroup.procs", "cgroup.events", "cgroup.kill", "cpu.stat"):
                if not (path / name).is_file():
                    raise UnsupportedIsolation(f"cgroup_control_missing:{name}")
            self.cpu_usage_us()
            return self
        except (OSError, ValueError) as error:
            self._cleanup()
            raise UnsupportedIsolation(f"cgroup_setup_failed:{error}") from error
        except BaseException:
            self._cleanup()
            raise

    def join_self(self):
        (self.path / "cgroup.procs").write_text(str(os.getpid()))
        membership = Path("/proc/self/cgroup").read_text().splitlines()
        if not any(line.startswith("0::") and line.endswith(f"/{self.path.name}")
                   for line in membership):
            raise OSError("cgroup_membership_unverified")

    def cpu_usage_us(self):
        try:
            stat = (self.path / "cpu.stat").read_text().splitlines()
            values = dict(line.split(maxsplit=1) for line in stat)
            return int(values["usage_usec"])
        except (OSError, KeyError, ValueError) as error:
            raise UnsupportedIsolation(f"cgroup_cpu_stat_unavailable:{error}") from error

    def _cleanup(self):
        if self.path is None:
            return
        path = self.path
        try:
            if (path / "cgroup.kill").exists():
                (path / "cgroup.kill").write_text("1")
            deadline = time.monotonic() + 5
            while (path / "cgroup.events").exists() and "populated 1" in (path / "cgroup.events").read_text():
                if time.monotonic() >= deadline:
                    raise UnsupportedIsolation("cgroup_cleanup_timeout")
                time.sleep(0.05)
            path.rmdir()
        except OSError as error:
            raise UnsupportedIsolation(f"cgroup_cleanup_failed:{error}") from error
        finally:
            self.path = None

    def __exit__(self, _kind, _error, _traceback):
        self._cleanup()


def probe_cgroup():
    """Verify the aggregate boundary can be provisioned before scoring."""
    with CgroupScope() as scope:
        return {"path": str(scope.path.parent), "controllers": ["cpu", "memory", "pids"],
                "cpu_max": f"{LIMITS['aggregate_cpu_quota_us']} {LIMITS['aggregate_cpu_period_us']}",
                "cpu_budget_us": LIMITS["aggregate_cpu_seconds"] * 1_000_000,
                "memory_max": LIMITS["address_bytes"], "pids_max": LIMITS["processes"]}


def capture(argv, seccomp_fd):
    """Run only after aggregate controls exist; always reap the whole cgroup."""
    with CgroupScope() as scope:
        return _capture_in_scope(argv, seccomp_fd, scope)


def _capture_in_scope(argv, seccomp_fd, scope):
    def before_exec():
        _limits()
        scope.join_self()

    child = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             pass_fds=(seccomp_fd,), preexec_fn=before_exec,
                             start_new_session=True, close_fds=True)
    selector = selectors.DefaultSelector()
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    selector.register(child.stdout, selectors.EVENT_READ, "stdout")
    selector.register(child.stderr, selectors.EVENT_READ, "stderr")
    deadline = time.monotonic() + LIMITS["wall_seconds"]
    reason = None
    try:
        while selector.get_map():
            if scope.cpu_usage_us() >= LIMITS["aggregate_cpu_seconds"] * 1_000_000:
                reason = "aggregate_cpu_limit"
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                reason = "wall_timeout"
                break
            for key, _ in selector.select(min(remaining, 0.5)):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                target = buffers[key.data]
                target.extend(chunk)
                if len(target) > LIMITS["output_bytes"]:
                    reason = "output_limit"
                    break
            if reason:
                break
        if reason:
            os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=5)
    finally:
        if child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=5)
        selector.close()
        child.stdout.close()
        child.stderr.close()
    return {"exit_code": child.returncode, "reason": reason,
            "stdout": bytes(buffers["stdout"][:LIMITS["output_bytes"]]),
            "stderr": bytes(buffers["stderr"][:LIMITS["output_bytes"]])}


def probe():
    """Exercise namespace, seccomp, and Landlock before any candidate launch."""
    if sys.platform != "linux":
        raise UnsupportedIsolation("linux_required")
    cgroup = probe_cgroup()
    if not shutil.which("bwrap"):
        raise UnsupportedIsolation("bubblewrap_missing")
    abi = landlock_abi()
    fd = export_seccomp_fd()
    try:
        # The probe's mount contains this public helper only, never the bench
        # repository or its private oracle.
        with tempfile.TemporaryDirectory(prefix="cgx2-isolation-probe-") as directory:
            root = Path(directory)
            shutil.copyfile(Path(__file__), root / "isolation.py")
            root.chmod(0o755)
            (root / "isolation.py").chmod(0o644)
            code = ("import sys,socket,subprocess;sys.path.insert(0,'/workspace');"
                    "import isolation;"
                    "assert not __import__('os').path.exists('/home');"
                    "isolation.install_exec_landlock('/usr/bin/python3');"
                    "\ntry: socket.socket(); raise RuntimeError('socket_allowed')"
                    "\nexcept PermissionError: pass"
                    "\ntry: subprocess.run(['/usr/bin/id'], check=True); raise RuntimeError('exec_allowed')"
                    "\nexcept PermissionError: pass")
            argv = profile_argv(root, root, fd, ["/usr/bin/python3", "-c", code])
            observed = capture(argv, fd)
        if observed["reason"]:
            raise UnsupportedIsolation(f"probe_{observed['reason']}")
        if observed["exit_code"] != 0:
            detail = observed["stderr"].decode("utf-8", "replace").strip()
            if "No permissions to create a new namespace" in detail:
                raise UnsupportedIsolation("user_namespace_unavailable")
            raise UnsupportedIsolation(f"namespace_or_sandbox_probe_failed:{detail[:300]}")
        return {"cgroup": cgroup, "landlock_abi": abi, "bubblewrap": shutil.which("bwrap"),
                "bubblewrap_sha256": file_sha256(shutil.which("bwrap")),
                "seccomp": "network_and_privileged_syscalls_denied",
                "network": "namespace_and_seccomp", "mounts": "allowlist_read_only",
                "runtime": "landlock_execute_and_process_seccomp"}
    finally:
        os.close(fd)


def profile_evidence():
    return {"version": PROFILE_VERSION, "limits": LIMITS,
            "aggregate_isolation": "dedicated cgroup v2 cpu, memory, pids with kill and cleanup",
            "outer_seccomp_denied": NETWORK_SYSCALLS + PRIVILEGED_SYSCALLS,
            "runtime_seccomp_denied": NETWORK_SYSCALLS + PRIVILEGED_SYSCALLS + PROCESS_SYSCALLS,
            "landlock": "execute only result and ELF interpreter",
            "mounts": ["/usr ro", "/bin ro", "/lib ro", "/lib64 ro",
                       "/etc/alternatives ro", "/etc/ld.so.cache ro",
                       "/toolchain ro", "/workspace ro", "/scratch tmpfs", "/tmp tmpfs"],
            "namespaces": "user,mount,pid,network,ipc,uts",
            "uid_gid": 65534, "capabilities": "dropped", "environment":
            {"PATH": "/toolchain/bin:/usr/bin:/bin", "HOME": "/scratch",
             "TMPDIR": "/scratch", "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"}}
