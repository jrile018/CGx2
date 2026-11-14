#!/usr/bin/env python3
"""Offline evidence integrity + fresh actual-source extraction/Lean replay.

Default performs the real commands. --evidence-only checks archived receipts and
hashes only; it NEVER claims a new extraction/proof run. No network commands.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tool-root', type=Path, default=Path('/tmp/cgx2-arch-review-proof'))
    parser.add_argument('--repo', type=Path)
    parser.add_argument('--evidence-only', action='store_true')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    repo = (args.repo or HERE.parents[2]).resolve()
    pins = json.loads((HERE / 'pins.json').read_text())
    receipt = json.loads((HERE / 'result.json').read_text())
    checks = []
    def note(id, status, scope, **fields):
        checks.append(dict(id=id, status=status, scope=scope, **fields))
        print(f'{status.upper()} {id}: {scope}')
    def require(ok, id, scope):
        note(id, 'pass' if ok else 'fail', scope)
        if not ok:
            raise RuntimeError(id)
    def run(id, argv, env, cwd, log, expected=0):
        log.parent.mkdir(parents=True, exist_ok=True)
        with log.open('w') as output:
            proc = subprocess.run(argv, env=env, cwd=cwd, stdout=output, stderr=subprocess.STDOUT)
        status = 'pass' if proc.returncode == expected else 'fail'
        note(id, status, 'fresh execution; ' + id, command=argv, exit_code=proc.returncode, evidence_file=str(log))
        if status != 'pass':
            raise RuntimeError(f'{id}; see {log}')
        return log.read_text()
    try:
        for rel, expected in pins['durable_sha256'].items():
            require(sha(HERE / rel) == expected, 'artifact:' + rel, 'archived artifact integrity only')
        manifest = json.loads((repo / 'research/architecture-review/input-manifest.json').read_text())
        source_rel = 'crates/cgx2-runtime/src/budget.rs'
        record = next(x for x in manifest['baseline_files'] if x['path'] == source_rel)
        source = (repo / source_rel).read_bytes()
        require(hashlib.sha256(source).hexdigest() == record['sha256'] == pins['source_sha256'],
                'source_binding', 'actual committed budget.rs matches frozen baseline manifest')
        mapping = json.loads((HERE / 'source/kernel-source-map.json').read_text())
        selected = b''.join(source[x['start_byte']:x['end_byte_exclusive']] for x in mapping['spans'])
        require(selected == (HERE / 'source/kernel-source.rs').read_bytes(), 'byte_exact_selection',
                'Budget + impl Session are concatenated unchanged source byte spans; guard impls omitted')
        for span in mapping['spans']:
            require(hashlib.sha256(source[span['start_byte']:span['end_byte_exclusive']]).hexdigest() == span['sha256'],
                    'span:' + str(span['start_byte']), 'source span hash')
        prior = repo / 'formal/memory-guarantees/NoGc.lean'
        require(sha(prior) == pins['historical_model_sha256'], 'historical_model_identity',
                'existing 13-theorem Nat model provenance; NOT a fresh Lean4.19 check or implementation proof')
        for check in receipt['checks']:
            if check.get('evidence_file'):
                require((HERE / check['evidence_file']).is_file(), 'receipt:' + check['id'], 'saved command evidence exists')
        if args.evidence_only:
            note('fresh_execution', 'not_run', 'evidence-only mode; no extraction or Lean command replayed')
        else:
            tool = args.tool_root.resolve()
            require(tool == Path(pins['tool_root']), 'fixed_replay_root',
                    'exact generated source comments require the pinned /tmp paths')
            for rel, expected in pins['tool_sha256'].items():
                require((tool / rel).is_file() and sha(tool / rel) == expected, 'tool:' + rel,
                        'isolated pinned compiler/translator executable integrity')
            env = os.environ.copy()
            env.update(RUSTUP_HOME=str(tool / 'rustup'), CARGO_HOME=str(tool / 'cargo'), LEAN_NUM_THREADS='2',
                       CARGO_BUILD_JOBS='2', PATH=str(tool / 'lean/bin') + os.pathsep + env['PATH'])
            replay = tool / 'offline-replay'
            replay.mkdir(exist_ok=True)
            (tool / 'kernel-source.rs').write_bytes(selected)
            (tool / 'kernel-projection.rs').write_bytes((HERE / 'source/kernel-projection.rs').read_bytes())
            (replay / 'Admission.lean').write_bytes((HERE / 'source/Admission.lean').read_bytes())
            run('charon', [str(tool/'charon'), 'rustc', '--preset', 'aeneas', '--dest', str(replay), '--',
                '--crate-name', 'budget_kernel', '--crate-type', 'lib', '--edition=2021', '-C', 'overflow-checks=yes',
                '--target', 'x86_64-unknown-linux-gnu', str(tool/'kernel-projection.rs')], env, replay, replay/'charon.log')
            llbc = json.loads((replay / 'budget_kernel.llbc').read_text())
            target = llbc['translated']['target_information'][0]
            require(target['key'] == 'x86_64-unknown-linux-gnu' and target['value']['target_pointer_size'] == 8
                    and llbc['translated']['runtime_checks']['overflow_checks'], 'machine_target',
                    'x86_64 little-endian 64-bit usize, Rust overflow checks enabled')
            llbc['translated']['options']['dest_dir'] = None
            normalized = hashlib.sha256(json.dumps(llbc, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
            require(normalized == pins['generated_llbc_normalized_sha256'], 'generated_llbc_identity',
                    'fresh extraction matches archived LLBC after removing destination-directory metadata only')
            run('aeneas', [str(tool/'aeneas'), '-backend', 'lean', '-dest', str(replay),
                str(replay/'budget_kernel.llbc')], env, tool, replay/'aeneas.log')
            require(sha(replay / 'BudgetKernel.lean') == pins['generated_lean_sha256'], 'generated_lean_identity',
                    'fresh translator output byte-identical; generated artifact was not edited')
            lake = str(tool/'lean/bin/lake')
            cwd = tool / 'backends/lean'
            run('lean_generated_kernel', [lake, 'env', 'lean', '-DwarningAsError=true', '-R', str(replay),
                '-o', str(replay/'BudgetKernel.olean'), str(replay/'BudgetKernel.lean')], env, cwd, replay/'generated-lean.log')
            env['LEAN_PATH'] = str(replay)
            proof_log = run('lean_source_connected_lemmas', [lake, 'env', 'lean', '-DwarningAsError=true', '-R', str(replay),
                str(replay/'Admission.lean')], env, cwd, replay/'proof.log')
            require('sorryAx' not in proof_log and len(re.findall(r'depends on axioms:', proof_log)) == 6,
                    'axiom_audit', 'six theorem audits; only propext, Classical.choice and Quot.sound permitted')
            for row in re.findall(r'depends on axioms: \[([^]]*)\]', proof_log):
                require(set(row.split(', ')) <= {'propext', 'Classical.choice', 'Quot.sound'}, 'allowed_axioms', row)
            require(proof_log.startswith('64\n'), 'lean_runtime_width',
                    'Lean executable evaluates opaque System.Platform.numBits to64; correspondence remains explicit assumption')
            # Fresh source-level negative control: reject the previously admitted exact-boundary case.
            neg = replay / 'negative'
            neg.mkdir(exist_ok=True)
            (neg/'kernel-source.rs').write_bytes(selected.replace(b'if bytes > budget.limit', b'if bytes >= budget.limit'))
            harness = (tool/'kernel-projection.rs').read_text().replace(str(tool/'kernel-source.rs'), str(neg/'kernel-source.rs'))
            (neg/'kernel-projection.rs').write_text(harness)
            (neg/'Admission.lean').write_bytes((HERE/'source/Admission.lean').read_bytes())
            run('negative_charon', [str(tool/'charon'), 'rustc', '--preset', 'aeneas', '--dest', str(neg), '--',
                '--crate-name', 'budget_kernel', '--crate-type', 'lib', '--edition=2021', '-C', 'overflow-checks=yes',
                '--target', 'x86_64-unknown-linux-gnu', str(neg/'kernel-projection.rs')], env, tool, neg/'charon.log')
            run('negative_aeneas', [str(tool/'aeneas'), '-backend', 'lean', '-dest', str(neg), str(neg/'budget_kernel.llbc')],
                env, tool, neg/'aeneas.log')
            run('negative_generated_compiles', [lake,'env','lean','-R',str(neg),'-o',str(neg/'BudgetKernel.olean'),
                str(neg/'BudgetKernel.lean')],env,cwd,neg/'generated.log')
            env['LEAN_PATH'] = str(neg)
            negative_log = run('negative_proof_rejected', [lake,'env','lean','-DwarningAsError=true','-R',str(neg),
                str(neg/'Admission.lean')],env,cwd,neg/'proof.log',expected=1)
            require('omega could not prove the goal' in negative_log, 'source_mutation_sensitivity',
                    'changing actual source > to >= produces compiling translation but breaks generated admission theorem')
        note('coverage_limit', 'blocked', 'whole Vec traversal, reachable-state induction, Session context/guard entry/RAII remain unproved')
        code = 0
    except (OSError, ValueError, RuntimeError, StopIteration) as exc:
        note('verification', 'fail', str(exc))
        code = 1
    if args.report:
        args.report.write_text(json.dumps({'status':'pass' if code == 0 else 'fail','evidence_only':args.evidence_only,'checks':checks}, indent=2)+'\n')
    print('Scope: source-selected generated loop bodies + conditional machine arithmetic; historical model and full runtime stay separate.')
    return code

if __name__ == '__main__':
    sys.exit(main())
