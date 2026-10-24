"""POSIX process-group supervisor with strictly bounded retained pipe output.

Limits apply per process, not to aggregate memory/CPU or escaped sessions.
Only the direct child is waitable; same-group descendants are killed on exit.
"""
import base64
import hashlib
import os
import selectors
import signal
import subprocess
import time


def supervise(command, *, cwd, env, limits, cap, timeout, merged=False):
    """Retain at most cap bytes per stream (one combined stream if merged).

    Read at most the remaining allowance plus one overflow-detection byte.
    No child output is written to disk. Kernel pipes provide backpressure.
    """
    started = time.monotonic()
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    reason, overflow_stream = None, None
    child = None
    killed = False

    def kill_group():
        nonlocal killed
        if not killed:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            killed = True

    try:
        child = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT if merged else subprocess.PIPE,
                                 start_new_session=True, preexec_fn=limits)
    except (OSError, subprocess.SubprocessError) as error:
        return {"exit": None, "reason": "spawn_error", "error": str(error),
                "stdout": b"", "stderr": b"", "duration_seconds": time.monotonic() - started}
    try:
        with selectors.DefaultSelector() as selector:
            for name, pipe in [("stdout", child.stdout), ("stderr", child.stderr)]:
                if pipe is not None:
                    os.set_blocking(pipe.fileno(), False)
                    selector.register(pipe, selectors.EVENT_READ, name)
            cleanup_deadline = None
            while selector.get_map() or child.poll() is None:
                now = time.monotonic()
                if reason is None and now - started >= timeout:
                    reason = "timeout"
                    kill_group()
                if child.poll() is not None or reason is not None:
                    kill_group()
                    if cleanup_deadline is None:
                        cleanup_deadline = now + 1
                    if now >= cleanup_deadline:
                        # An escaped process may still hold a pipe. Never wait forever.
                        reason = reason or "pipe_cleanup_timeout"
                        break
                for key, _ in selector.select(min(0.02, max(0, timeout - (now - started))) if reason is None else 0.02):
                    data = os.read(key.fileobj.fileno(), min(65536, cap - len(buffers[key.data]) + 1))
                    if not data:
                        selector.unregister(key.fileobj)
                        key.fileobj.close()
                        continue
                    remaining = cap - len(buffers[key.data])
                    buffers[key.data].extend(data[:remaining])
                    if len(data) > remaining and reason is None:
                        reason, overflow_stream = "output_overflow", key.data
                        kill_group()
                if reason is not None:
                    break
                if not selector.get_map() and child.poll() is None:
                    time.sleep(0.005)
    finally:
        kill_group()
        child.wait()
        for pipe in [child.stdout, child.stderr]:
            if pipe is not None:
                pipe.close()
    return {"exit": child.returncode,
            "reason": reason or ("signal" if child.returncode < 0 else "exit"),
            "signal": -child.returncode if child.returncode < 0 else None,
            "overflow_stream": overflow_stream,
            "stdout": bytes(buffers["stdout"]), "stderr": bytes(buffers["stderr"]),
            "duration_seconds": round(time.monotonic() - started, 3)}


def evidence(result, *, include_output=True):
    """JSON-safe evidence; preserve undecodable bytes exactly as base64."""
    record = {key: value for key, value in result.items() if key not in ("stdout", "stderr")}
    for name in ("stdout", "stderr"):
        data = result[name]
        record[name + "_bytes"] = len(data)
        record[name + "_sha256"] = hashlib.sha256(data).hexdigest()
        if include_output:
            record[name] = data.decode("utf-8", errors="replace")
            try:
                data.decode("utf-8")
            except UnicodeDecodeError:
                record[name + "_base64"] = base64.b64encode(data).decode("ascii")
    return record
