#!/usr/bin/env python3
"""Independently reconcile actual rows; unrelated failures remain failures."""
import json
from pathlib import Path
import re
import sys
from common import ROOT, digest, normal, observed, stable
from run import roots_ok

scratch = Path(sys.argv[1]).resolve()


def read(path):
    return json.loads((scratch / path).read_text())


def counts(row):
    matches = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', row['stdout'])
    return dict(zip(('passed', 'failed', 'ignored'), (sum(int(m[i]) for m in matches) for i in range(3))))


verification = read('verification-01/report.json')
commands = {r['name']: r for r in verification['commands']}
initial = counts(commands['workspace-tests'])
transport = read('retry-transport/report.json')['commands'][0]
http = read('retry-http/report.json')['commands'][0]
assert normal(transport) and normal(http)
assert counts(transport) == dict(passed=11, failed=0, ignored=0)
assert counts(http) == dict(passed=24, failed=0, ignored=0)
assert all(normal(commands[k]) for k in ('clippy', 'fmt', 'build'))
for name in ('frozen-runtime-tests-build', 'frozen-runtime-tests-run', 'frozen-runtime-clippy', 'harness-selftests'):
    assert normal(read(name + '/report.json')['commands'][0]), name
assert counts(read('frozen-runtime-tests-run/report.json')['commands'][0]) == dict(passed=4, failed=0, ignored=0)
red_green = read('hashed-red-green/report.json')
assert red_green['complete'] and red_green['failed'] == 0 and not red_green['exceptions']

products = read('products-02/report.json')
assert products['complete'] and not products['exceptions']
assert all(normal(r, 1 if r['name'] in ('matrix', 'historical', 'api', 'recovery') else 0) for r in products['commands'])
api_attempt = next(r for r in products['commands'] if r['name'] == 'api')
assert 'FileExistsError' in api_attempt['stderr'] and 'product-api-01' in api_attempt['stderr']
assert normal(read('api-retry-03/report.json')['commands'][0])
matrix = read('products-02/matrix-01/report.json')
failures = [r['name'] for r in matrix['checks'] if not r['passed']]
assert failures == ['direct:copy-budget-exact', 'dependency-negative-exact-file'], failures
assert matrix['complete'] and not matrix['exceptions']
assert matrix['frozen_before'] == matrix['frozen_after']
rows = {r['name']: r for r in matrix['commands']}
allowed = {'-10\n', '0\n', '-1\n', '-2\n', '-3\n', '-4\n', '-5\n', '42\n'}
for index in range(1, 65):
    row = rows[f'source-reserve-{index}']
    assert normal(row) and row['stdout'] in allowed, index
    assert observed(row) == {'session_entered': False, 'metrics': None} if index == 1 else roots_ok(observed(row))
for name, row in rows.items():
    if name.startswith('source-fault-') and name.endswith('-run'):
        assert normal(row) and row['stdout'] in allowed, name

summaries = {}
for name, expected in [('auto', 29), ('api', 84), ('admission', 196), ('package', 40), ('review', 15)]:
    report = read('api-03/report.json' if name == 'api' else f'products-02/{name}-01/report.json')
    assert report['passed'] == expected and report['failed'] == 0, (name, report.get('passed'), report.get('failed'))
    summaries[name] = dict(passed=report['passed'], failed=report['failed'])
historical = read('products-02/historical-01/product/product.json')
assert historical['passed'] == 135 and historical['failed'] == 1
assert [c['name'] for c in historical['checks'] if not c['passed']] == ['regression-managed-product']
regressions = [r for r in historical['commands'] if r['name'].startswith('regression-')]
assert len(regressions) == 18
preliminary_recovery = read('products-02/recovery-01/report.json')
assert [c['name'] for c in preliminary_recovery['checks'] if not c['passed']] == [
    'return-alias-interpreter:outcome', 'return-alias-interpreter:cleanup',
    'return-alias-debug-build', 'return-alias-release-build', 'return-alias-standalone-emit']
assert 'CGT095' in next(c for c in preliminary_recovery['commands'] if c['name'] == 'return-alias-interpreter')['stderr']
recovery = read('recovery-02/report.json')
assert recovery['complete'] and recovery['failed'] == 0 and not recovery['exceptions']
assert sum(r['name'].startswith('derived:') for r in recovery['checks']) == 50
legacy = read('legacy-reconciliation-02/report.json')
assert legacy['complete'] and legacy['failed'] == 0 and not legacy['exceptions']
assert sum(c['name'].startswith('derived:') for c in legacy['checks']) == 120
legacy_raw = read('products-02/historical-01/product/regressions/managed-product/managed-product/repair/repair.json')
assert legacy_raw['passed'] == 670 and legacy_raw['failed'] == 8
prior_legacy = read('historical-01/product/regressions/managed-product/managed-product/repair/repair.json')
assert legacy['original_report_sha256'] == digest(scratch / 'historical-01/product/regressions/managed-product/managed-product/repair/repair.json')
assert stable(legacy_raw['outcomes']) == stable(prior_legacy['outcomes'])
assert digest(ROOT / 'crates/cgx2-syntax/ordinary-recovery-roots-evidence/fixtures/sweep.cg') == digest(ROOT / 'crates/cgx2-syntax/ordinary-parity-repair-evidence/fixtures/sweep.cg')

frozen = scratch / 'verification-01/frozen'
manifest = read('verification-01/frozen.json')
artifacts = dict(manifest['binary_sha256'], **manifest['probe_library_sha256'])
assert artifacts == {p: digest(frozen / p) for p in artifacts}
assert manifest['dependency_sha256'] == {p.name: digest(p) for p in (frozen / 'deps').iterdir()
    if p.is_file() and p.suffix in ('.rlib', '.rmeta', '.so')}
inputs = read('verification-01/source.json')
assert inputs == {p: digest(ROOT / p) for p in inputs}
assert all(r['passed'] for r in products['checks'] if r['name'] not in ('matrix', 'historical', 'api', 'recovery'))

contracts = ['AGENTS.md', 'docs/language-design.md', 'docs/validation-plan.md', 'plans/roadmap.md',
    'plans/wave28/README.md', 'plans/wave28/REVIEW.md', 'plans/release-completion/README.md',
    'plans/release-completion/contracts.md', 'plans/release-completion/worker-rules.md',
    'plans/release-completion/dispatch.md', 'plans/release-completion/s2b-ordinary-memory/plan.md',
    'plans/release-completion/s2b-ordinary-memory-review/README.md',
    'plans/release-completion/s2b-ordinary-memory-review/recovery-roots/plan.md']
toolchain = matrix['toolchain_files']
for name in list(toolchain):
    if name.endswith('/bin/rustc'):
        cargo = Path(name).with_name('cargo')
        toolchain[str(cargo)] = digest(cargo)
        driver = Path(name).with_name('clippy-driver')
        toolchain[str(driver)] = digest(driver)

result = dict(schema_version=1, reviewed_base='89464a76c9bd84a3b090ec58a14ca6be0043c6ea',
    disposition='bounded recovery repair verified; broad memory/release gates remain open',
    ordinary_memory_accepted=False, release_accepted=False,
    initial_workspace=initial, permission_retries=dict(transport=counts(transport), http=counts(http)),
    workspace_unique_covered=dict(passed=initial['passed'] - 1 + 11 + 24, failed=0, ignored=initial['ignored']),
    focused=dict(runtime_frozen=4, sema=8, total=12,
        note='One extra reassignment test added after freeze, compiled with rustc and linted with clippy-driver against the frozen library; no Cargo after freeze.'),
    hashed_red_green=dict(passed=red_green['passed'], failed=red_green['failed'],
        test_source_sha256=red_green['test_source_sha256'],
        test_artifact_sha256=red_green['test_artifact_sha256'],
        baseline_library_sha256=red_green['baseline_library_sha256'],
        canonical_runtime_sha256=red_green['canonical_runtime_sha256']),
    matrix=dict(passed=matrix['passed'], failed=matrix['failed'], failures=failures),
    products=summaries, historical=dict(passed=135, failed=1, regression_commands=18,
        nested_stale_fault_checks=dict(passed=670, failed=8),
        derived_reconciliation=dict(passed=legacy['passed'], failed=legacy['failed'], traces=legacy['traces'])),
    recovery=dict(passed=recovery['passed'], failed=recovery['failed'], trace=recovery['unaffected_reserve_trace'],
                  derived_copy_budget_controls=50, source_injections=64,
                  canonical_native_fault_runs=sum(n.startswith('source-fault-') and n.endswith('-run') for n in rows)),
    immutable=dict(artifacts=len(artifacts), dependencies=len(manifest['dependency_sha256']),
                   compiler_inputs=len(inputs), artifacts_unchanged=True, dependencies_unchanged=True, source_unchanged=True),
    toolchain_sha256=toolchain, contract_sha256={p:digest(ROOT/p) for p in contracts},
    frozen_test_binary_sha256={name:digest(ROOT/'target/release-ordinary-recovery-roots/debug/deps'/name)
        for name in ('transport-e8ffa26d770d4480', 'http-e86675e588a1e047')},
    prior_review_sha256=digest(ROOT/'plans/release-completion/s2b-ordinary-memory-review/evidence/combined-reviewed/acceptance.json'),
    remaining=matrix['unsupported_required'] + ['Nominal borrow constructor resolution',
        'Existing harness completeness', 'Precise static dependency filename', 'Historical stale375 copy assertion',
        'Eight stale fixed-reserve assertions in unchanged managed-product regression; raw135/1 retained with independent reached-site reconciliation'],
    attempts=dict(first_product='retained quota failures, including partial copies archived before own tmp cleanup',
                  preliminary_tests='all authoring mistakes and base failures retained; no failure relabeled as a pass'))
(scratch / 'audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
