#!/usr/bin/env python3
"""Verify frozen inputs and checkpoint evidence; does not prove CGx2 correct."""
import argparse
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--inputs-only', action='store_true')
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[1]
manifest = json.loads((here / 'input-manifest.json').read_text())

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

for item in manifest['baseline_files']:
    path = repo / item['path']
    assert path.is_file() and sha(path) == item['sha256'], ('baseline drift', item['path'])
snapshot = Path(manifest['wip_snapshot_root'])
if not snapshot.is_dir() and manifest.get('durable_wip_snapshot_root'):
    snapshot = Path(manifest['durable_wip_snapshot_root'])
for item in manifest['wip_files']:
    path = snapshot / item['path']
    assert path.is_file() and sha(path) == item['sha256'], ('snapshot drift', item['path'])
summary = {'baseline_files': len(manifest['baseline_files']),
           'wip_files': len(manifest['wip_files']), 'source_integrity': 'passed'}
if args.inputs_only:
    print(json.dumps(summary))
    raise SystemExit(0)

counts = {}
for lane in ['architecture', 'proof', 'environment', 'comparisons']:
    root = here / lane
    result = json.loads((root / 'result.json').read_text())
    assert result['status'] in {'complete', 'partial', 'blocked'}, lane
    assert isinstance(result['scope'], str) and result['scope'], lane
    assert isinstance(result['checks'], list) and result['checks'], lane
    assert isinstance(result['blockers'], list), lane
    states = {}
    seen = set()
    for check in result['checks']:
        assert check['id'] not in seen, (lane, 'duplicate check')
        seen.add(check['id'])
        status = check['status']
        assert status in {'pass', 'fail', 'blocked', 'not_run'}, (lane, check['id'])
        assert check['scope'], (lane, check['id'])
        assert check['command'] or status == 'not_run', (lane, check['id'])
        states[status] = states.get(status, 0) + 1
        evidence = check.get('evidence_file')
        if evidence:
            path = Path(evidence)
            if not path.is_absolute():
                path = root / path
            assert path.is_file(), (lane, 'missing evidence', evidence)
        if status == 'pass':
            assert check['exit_code'] == 0 and evidence, (lane, check['id'])
    for blocker in result['blockers']:
        assert blocker['id'] and blocker['description'] and blocker['remedies'], lane
    assert (root / ('protocol.md' if lane == 'comparisons' else 'report.md')).is_file(), lane
    counts[lane] = {'receipt': result['status'], 'checks': states}

claims_document = json.loads((here / 'architecture/claims.json').read_text())
assert claims_document['baseline_commit'] == manifest['baseline_commit']
claims = claims_document['claims']
assert isinstance(claims, list) and len(claims) >= 10
seen = set()
allowed = {'intended', 'implemented', 'implemented_unaccepted', 'historically_tested',
           'model_proved', 'implementation_connected', 'blocked'}
for claim in claims:
    assert claim['id'] not in seen
    seen.add(claim['id'])
    assert claim['name'] and claim['scope'] and claim['status'] in allowed, claim['id']
    for field in ['source_refs', 'assumptions', 'obligations', 'evidence']:
        assert isinstance(claim[field], list), (claim['id'], field)
    for ref in claim['source_refs']:
        assert ref['variant'] in {'baseline', 'wip-overlay'}, ref
        root = repo if ref['variant'] == 'baseline' else snapshot
        path = root / ref['path']
        assert path.is_file(), ref
        length = len(path.read_text().splitlines())
        assert 1 <= ref['line_start'] <= ref['line_end'] <= length, ref

gpu_remedies_root = here / 'environment'
gpu_remedies = json.loads((gpu_remedies_root / 'gpu-remedies.json').read_text())
for item in gpu_remedies['executed_discovery']['evidence_files']:
    path = gpu_remedies_root / item['path']
    assert path.stat().st_size == item['bytes'] and sha(path) == item['sha256'], item['path']
assert sum(a['compressed_bytes'] for a in gpu_remedies['artifacts']) == gpu_remedies['footprint']['selected_four_artifacts_compressed_bytes']
assert gpu_remedies['footprint']['under_3gib_lane_cap'].startswith('NOT_PROVEN')
summary['gpu_remedies'] = 'metadata verified; installation and driver compatibility not established'

replay_root = here / 'verification/environment-replay'
replay = json.loads((replay_root / 'checks.json').read_text())
expected_replay_ids = {check['id'] for check in json.loads((here / 'environment/result.json').read_text())['checks']}
assert {check['id'] for check in replay} == expected_replay_ids
assert len(expected_replay_ids) == len(replay) == 14
assert sum(check['status'] == 'pass' for check in replay) == 13
assert sum(check['status'] == 'blocked' for check in replay) == 1
for check in replay:
    log = replay_root / check['evidence_file']
    assert log.is_file(), ('coordinator replay evidence missing', check['id'])
    assert 'exit_code=' + str(check['exit_code']) + '\n' in log.read_text()
    assert check['status'] in {'pass', 'blocked'}, check['id']
    if check['status'] == 'pass':
        assert check['exit_code'] == 0
    else:
        assert check['id'] == 'torch_gpu_attempt' and check['exit_code'] != 0
summary['coordinator_environment_replay'] = {
    status: sum(check['status'] == status for check in replay)
    for status in ['pass', 'blocked']
}

proof_replay_root = here / 'verification/proof-replay'
proof_replay = json.loads((proof_replay_root / 'checks.json').read_text())
assert proof_replay['status'] == 'pass' and proof_replay['evidence_only'] is False
executed = {c['id']: c for c in proof_replay['checks'] if c.get('command')}
expected_proof_codes = dict.fromkeys(['charon', 'aeneas', 'lean_generated_kernel',
    'lean_source_connected_lemmas', 'negative_charon', 'negative_aeneas',
    'negative_generated_compiles'], 0)
expected_proof_codes['negative_proof_rejected'] = 1
assert set(executed) == set(expected_proof_codes)
for ident, code in expected_proof_codes.items():
    check = executed[ident]
    assert check['status'] == 'pass' and check['exit_code'] == code
    assert (proof_replay_root / check['evidence_file']).is_file()
summary['coordinator_proof_replay'] = 'fresh source extraction, six bounded lemmas, expected-failure source mutation; receipts retained'

comparison = here / 'comparisons/matrix.json'
assert comparison.is_file(), 'comparison matrix missing'
if comparison.is_file():
    matrix = json.loads(comparison.read_text())
    assert set(matrix['languages']) == {'CGx2', 'Rust', 'OCaml', 'Python'}
    assert set(matrix['policies']) == {'idiomatic', 'equipped'}
    assert set(matrix['targets']) == {'cpu', 'gpu'}
    assert set(matrix['tracks']) == {'native', 'data_ml', 'general_llm'}
    assert matrix['cases'], 'comparison cases missing'
    summary['comparison_cases'] = len(matrix['cases'])
summary.update({'lane_receipts': counts, 'scoped_claims': len(claims),
                'interpretation': 'Artifact/source-integrity checks, not runtime or compiler correctness.'})
print(json.dumps(summary, indent=2))
