"""Live bounded command capture for this repair's evidence only."""
import hashlib
import os
from pathlib import Path
import resource
import signal
import subprocess
import time

OUTPUT_CAP = 1024 ** 2
FILE_CAP = 8 * OUTPUT_CAP
ARTIFACT_CAP = 512 * OUTPUT_CAP


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024 ** 3,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (900, 900))
    # Compiler artifacts exceed log limits. This finite inherited ceiling also
    # bounds file output between live monitor samples without truncating rlibs.
    resource.setrlimit(resource.RLIMIT_FSIZE, (ARTIFACT_CAP,) * 2)


def process_tree(root, known):
    processes = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
            processes[int(entry.name)] = (int(fields[1]), fields[19])
        except (OSError, ValueError, IndexError):
            pass
    alive = {pid for pid, stamp in known.items()
             if pid in processes and processes[pid][1] == stamp}
    alive.add(root)
    while True:
        children = {pid for pid, (parent, _) in processes.items() if parent in alive}
        if children <= alive:
            break
        alive |= children
    for pid in alive:
        if pid in processes:
            known[pid] = processes[pid][1]
    return alive & processes.keys()


def kill_tree(root, known):
    for pid in sorted(process_tree(root, known), reverse=True):
        try:
            # Each script may create its own sessions; kill every observed
            # descendant PID, never an unrelated shared process group.
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def run(command, cwd, env, log, timeout=900):
    command = list(map(str, command))
    log = Path(log)
    log.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    known = {}
    reason = None
    with log.open('wb') as output:
        child = subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                                 stderr=subprocess.STDOUT, start_new_session=True,
                                 preexec_fn=limits)
        while child.poll() is None:
            descendants = process_tree(child.pid, known)
            if time.monotonic() - started > timeout:
                reason = 'wall-time limit'
            elif len(descendants) > 128:
                reason = 'process-tree limit'
            elif log.stat().st_size > FILE_CAP:
                reason = 'aggregate log limit'
            else:
                for pid in descendants:
                    if pid == child.pid:
                        continue
                    for fd in (1, 2):
                        try:
                            info = os.stat(f'/proc/{pid}/fd/{fd}')
                            if info.st_size > OUTPUT_CAP:
                                reason = 'live descendant output limit'
                        except OSError:
                            pass
            if reason:
                kill_tree(child.pid, known)
                break
            time.sleep(0.05)
        child.wait()
        kill_tree(child.pid, known)
    data = log.read_bytes()
    return {'command': command, 'exit': child.returncode, 'limit': reason,
            'log': log.name, 'log_sha256': hashlib.sha256(data).hexdigest(),
            'duration_seconds': round(time.monotonic() - started, 3)}
