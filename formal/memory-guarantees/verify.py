#!/usr/bin/env python3
"""Check the pinned Lean proof and ensure weakened policies fail verification."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--lean', default=os.environ.get('LEAN', shutil.which('lean')))
parser.add_argument('--report', type=Path)
args = parser.parse_args()
if not args.lean:
    parser.error('Lean 4.19.0 is required; pass --lean /path/to/lean')
root = Path(__file__).resolve().parent
source = root / 'NoGc.lean'
text = source.read_text()
version = subprocess.check_output([args.lean, '--version'], text=True).strip()
if not re.search(r'version 4\.19\.0[,)]', version):
    raise SystemExit('Unexpected Lean version: ' + version)
if re.search(r'\bsorry\b|^\s*(axiom|unsafe|opaque)\b', text, re.MULTILINE):
    raise SystemExit('An unproved declaration or admitted proof was found')

def check(path):
    return subprocess.run([args.lean, '-DwarningAsError=true', str(path)],
                          cwd=root, text=True, capture_output=True, timeout=60)

base = check(source)
if base.returncode or 'sorryAx' in base.stdout + base.stderr:
    raise SystemExit('Proof failed:\n' + base.stdout + base.stderr)
# Break distinct obligations, rather than merely rerun the same assertions.
mutations = {
    'ignore_all_budgets': (
        'guards.all fun g => decide (g.spent + bytes ≤ g.limit)',
        'true'),
    'allow_collection_inside_guard': (
        'if s.guards.isEmpty then', 'if true then'),
    'ignore_inner_budgets': (
        'guards.all fun g => decide (g.spent + bytes ≤ g.limit)',
        '(guards.take 1).all fun g => decide (g.spent + bytes ≤ g.limit)'),
    'commit_bytes_on_failure': (
        'else .allocationError s',
        'else .allocationError { s with allocated := s.allocated + bytes }'),
}
expected_failures = {
    'ignore_all_budgets': 'execute_valid',
    'allow_collection_inside_guard': 'execute_no_gc',
    'ignore_inner_budgets': 'execute_valid',
    'commit_bytes_on_failure': 'rejected_allocation_unchanged',
}
results = {}
with tempfile.TemporaryDirectory(prefix='cgx2-lean-negative-') as tmp:
    for name, (before, after) in mutations.items():
        if text.count(before) != 1:
            raise SystemExit('Mutation anchor changed: ' + name)
        path = Path(tmp) / (name + '.lean')
        path.write_text(text.replace(before, after))
        result = check(path)
        # A failed proof is temporarily elaborated with sorryAx by Lean.
        # Require that diagnostic on the expected theorem, so an unrelated
        # linter warning, missing import or parse error cannot satisfy this check.
        output = result.stdout + result.stderr
        failed_obligation = any(
            'CGx2.MemoryGuarantees.' + expected_failures[name] + "'" in line
            and 'sorryAx' in line for line in output.splitlines())
        if result.returncode == 0 or not failed_obligation:
            raise SystemExit('Weakened policy was not rejected: ' + name)
        results[name] = 'rejected'
report = {
    'lean': version,
    'source': source.name,
    'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'theorem_count': len(re.findall(r'^theorem ', text, re.MULTILINE)),
    'boundary_examples': len(re.findall(r'^example ', text, re.MULTILINE)),
    'proof_check': 'passed',
    'warnings_as_errors': True,
    'axiom_audit': base.stdout.strip().splitlines(),
    'negative_policy_checks': results,
    'scope': 'Reference model only; no implementation refinement or language-wide dominance proof.',
}
if args.report:
    args.report.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
