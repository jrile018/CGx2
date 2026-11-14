#!/usr/bin/env python3
"""Read-only board. S/M/L=1/2/3 are scheduling weights, not durations.
All running/review entries count as local activity for measurement_host.
"""
import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SHA = re.compile(r'[0-9a-f]{40}\Z')
ACTIVE = {'running', 'review'}
STATUSES = {'pending', 'running', 'review', 'integrated', 'blocked'}
WEIGHTS = {'S': 1, 'M': 2, 'L': 3}
LEAF = ('You are a leaf worker. Do not spawn or delegate. Do the assigned work '
        'yourself and return the completed deliverable in this turn; do not return '
        'a promise of future results.')
PLAN = 'plans/lean-architecture-review/parallel/'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def text(value):
    return isinstance(value, str) and bool(value.strip())


def sha(value):
    return isinstance(value, str) and SHA.fullmatch(value) is not None


def strings(value, label, empty=False):
    require(isinstance(value, list) and (empty or bool(value)), f'{label}: expected list')
    require(all(text(v) for v in value), f'{label}: blank/non-string item')
    require(len(value) == len(set(value)), f'{label}: duplicate item')


def parts(value):
    require(text(value) and not value.startswith('/'), f'invalid owned path: {value!r}')
    require(all(p not in {'', '.', '..'} for p in value.rstrip('/').split('/'))
            and not any(c in value for c in '*?[]\\'), f'invalid owned prefix: {value!r}')
    return PurePosixPath(value).parts


def overlaps(a, b):
    a, b = parts(a), parts(b)
    return a[:len(b)] == b or b[:len(a)] == a


def branch_ok(value):
    return (text(value) and value.startswith('riley/') and len(value) > 6
            and not any(c.isspace() or ord(c) < 32 or c in '~^:?*[\\' for c in value)
            and not any(s in value for s in ('..', '//', '@{'))
            and all(p and not p.startswith('.') and not p.endswith(('.', '.lock'))
                    for p in value.split('/')))


def assignment(entry, label):
    require(type(entry.get('attempt')) is int and entry['attempt'] > 0, f'{label}: positive attempt required')
    require(text(entry.get('worktree')) and Path(entry['worktree']).is_absolute(), f'{label}: absolute worktree required')
    require(branch_ok(entry.get('branch')), f'{label}: invalid riley branch')
    require(sha(entry.get('base_commit')), f'{label}: exact base SHA required')


class Board:
    def __init__(self, spec, state):
        self.spec, self.state = spec, state
        self.validate()

    def ancestors(self, bid):
        result = set()
        for dep in self.blocks[bid]['depends_on']:
            result.add(dep)
            result.update(self.ancestors(dep))
        return result

    def conflicts(self, left, right):
        a, b = self.blocks[left], self.blocks[right]
        return ('measurement_host' in set(a['resource_locks']) | set(b['resource_locks'])
                or bool(set(a['resource_locks']) & set(b['resource_locks']))
                or any(overlaps(x, y) for x in a['owns'] for y in b['owns']))

    def validate(self):
        p, s = self.spec, self.state
        require(isinstance(p, dict) and isinstance(s, dict), 'expected JSON objects')
        require(p.get('schema_version') == s.get('schema_version') == 1, 'unsupported schema')
        require(sha(p.get('checkpoint_commit')) and sha(s.get('integration_head')), 'exact checkpoint/integration_head SHA required')
        require(type(p.get('max_workers')) is int and 1 <= p['max_workers'] <= 3, 'max_workers must be 1..3')
        require(branch_ok(p.get('integration_branch')), 'invalid integration branch')
        for field in ('source_plan', 'historical_ledger', 'result_root'):
            parts(p.get(field))
        for field in ('coordinator_owns', 'global_non_goals'):
            strings(p.get(field), field)
        for path in p['coordinator_owns']:
            parts(path)
        require(isinstance(p.get('blocks'), list) and bool(p['blocks']), 'schema requires nonempty blocks')
        self.blocks = {}
        for b in p['blocks']:
            require(isinstance(b, dict), 'block must be an object')
            bid = b.get('id')
            require(text(bid) and re.fullmatch(r'[A-Z][A-Z0-9]*', bid), 'invalid block ID')
            require(bid not in self.blocks, f'duplicate block ID: {bid}')
            self.blocks[bid] = b
            for field in ('title', 'goal', 'coordination_risk'):
                require(text(b.get(field)), f'{bid}: missing {field}')
            require(b.get('estimate') in WEIGHTS, f'{bid}: invalid estimate')
            require(type(b.get('wave')) is int and b['wave'] > 0, f'{bid}: invalid wave')
            for field in ('depends_on', 'gates', 'resource_locks', 'owns', 'consumes', 'produces', 'steps', 'acceptance', 'verification', 'non_goals'):
                strings(b.get(field), f'{bid}.{field}', field in {'depends_on', 'gates', 'resource_locks'})
            for path in b['owns'] + b['produces']:
                parts(path)
            require(all(any(parts(path)[:len(parts(owner))] == parts(owner) for owner in b['owns'])
                        for path in b['produces']), f'{bid}: output outside ownership')
            require(not any(overlaps(a, c) for a in b['owns'] for c in p['coordinator_owns']), f'{bid}: coordinator ownership conflict')
        visiting, visited = set(), set()
        def visit(bid):
            require(bid in self.blocks, f'missing dependency: {bid}')
            require(bid not in visiting, f'dependency cycle: {bid}')
            if bid in visited:
                return
            visiting.add(bid)
            for dep in self.blocks[bid]['depends_on']:
                visit(dep)
            visiting.remove(bid)
            visited.add(bid)
        for bid in self.blocks:
            visit(bid)
        require(isinstance(p.get('gates'), dict) and isinstance(s.get('gates'), dict), 'invalid gates')
        require(set(p['gates']) == set(s['gates']), 'gate state IDs differ')
        require(isinstance(s.get('blocks'), dict) and set(s['blocks']) == set(self.blocks), 'state block IDs must match spec exactly')
        for gid, gate in p['gates'].items():
            require(isinstance(gate, dict) and gate.get('owner') in {'user', 'coordinator'} and text(gate.get('meaning')), f'{gid}: invalid gate contract')
            strings(gate.get('requires'), f'{gid}.requires')
            require(set(gate['requires']) <= set(self.blocks), f'{gid}: missing required block')
            gs = s['gates'][gid]
            require(isinstance(gs, dict) and gs.get('status') in {'pending', 'approved', 'withheld'}, f'{gid}: invalid gate status')
            if gs['status'] == 'approved':
                require(text(gs.get('decision_ref')), f'{gid}: approved gate needs decision_ref')
        for bid, b in self.blocks.items():
            require(set(b['gates']) <= set(p['gates']), f'{bid}: unknown gate')
            for gid in b['gates']:
                require(set(p['gates'][gid]['requires']) <= self.ancestors(bid), f'{bid}: gate prerequisites must be dependencies')
            e = s['blocks'][bid]
            require(isinstance(e, dict) and e.get('status') in STATUSES, f'{bid}: invalid state')
            require({'status', 'attempt', 'worker_kind', 'worker_id', 'worktree', 'branch', 'base_commit',
                     'worker_commit', 'integration_commit', 'receipt', 'integration_receipt', 'blocker'} <= set(e),
                    f'{bid}: missing state fields')
            require(type(e.get('attempt')) is int and e['attempt'] >= 0, f'{bid}: invalid attempt')
            for field in ('worker_commit', 'integration_commit', 'base_commit'):
                require(e.get(field) is None or sha(e[field]), f'{bid}: invalid {field}')
            if e['status'] in ACTIVE:
                assignment(e, bid)
                require(e.get('worker_kind') in {'subagent', 'chat'} and text(e.get('worker_id')), f'{bid}: worker identity required')
                require(e['branch'] != p['integration_branch'], f'{bid}: worker cannot use integration branch')
            if e['status'] == 'review':
                require(sha(e.get('worker_commit')) and text(e.get('receipt')), f'{bid}: review requires worker SHA and receipt')
            if e['status'] == 'integrated':
                require(sha(e.get('integration_commit')) and text(e.get('integration_receipt')), f'{bid}: integrated requires exact integration SHA and receipt')
        for bid, e in s['blocks'].items():
            if e['status'] in ACTIVE | {'integrated'}:
                require(not self.unfulfilled(bid), f'{bid}: unmet dependencies/gates: {self.unfulfilled(bid)}')
        ids = list(self.blocks)
        for i, left in enumerate(ids):
            for right in ids[i + 1:]:
                if any(overlaps(a, b) for a in self.blocks[left]['owns'] for b in self.blocks[right]['owns']):
                    require(left in self.ancestors(right) or right in self.ancestors(left), f'unordered shared ownership: {left}, {right}')
        active = self.active()
        require(len(active) <= p['max_workers'], 'too many active workers')
        for i, left in enumerate(active):
            for right in active[i + 1:]:
                require(not self.conflicts(left, right), f'active ownership/lock conflict: {left}, {right}')
                a, b = s['blocks'][left], s['blocks'][right]
                require(Path(a['worktree']).resolve() != Path(b['worktree']).resolve()
                        and a['branch'] != b['branch'] and a['worker_id'] != b['worker_id'], f'shared active assignment: {left}, {right}')

    def active(self):
        return [bid for bid, e in self.state['blocks'].items() if e['status'] in ACTIVE]

    def unfulfilled(self, bid):
        b = self.blocks[bid]
        return ([f"dependency {d}: {self.state['blocks'][d]['status']}" for d in b['depends_on'] if self.state['blocks'][d]['status'] != 'integrated']
                + [f"gate {g}: {self.state['gates'][g]['status']}" for g in b['gates'] if self.state['gates'][g]['status'] != 'approved'])

    def reasons(self, bid):
        reasons = self.unfulfilled(bid)
        status = self.state['blocks'][bid]['status']
        if status != 'pending':
            reasons.append(f'status is {status}, not pending')
        if len(self.active()) >= self.spec['max_workers']:
            reasons.append('all worker slots occupied')
        reasons.extend(f'active conflict with {a}' for a in self.active() if a != bid and self.conflicts(bid, a))
        return reasons

    def critical_path(self, bid):
        weight = 0 if self.state['blocks'][bid]['status'] == 'integrated' else WEIGHTS[self.blocks[bid]['estimate']]
        return weight + max((self.critical_path(child) for child, b in self.blocks.items() if bid in b['depends_on']), default=0)

    def ready(self):
        eligible = sorted((bid for bid in self.blocks if not self.reasons(bid)), key=lambda bid: (-self.critical_path(bid), bid))
        shortlist = []
        for bid in eligible:
            if len(shortlist) < self.spec['max_workers'] - len(self.active()) and not any(self.conflicts(bid, other) for other in shortlist):
                shortlist.append(bid)
        return {'eligible': [{'id': b, 'remaining_weight': self.critical_path(b)} for b in eligible],
                'dispatch_shortlist': shortlist, 'ranking': 'Longest remaining successor path; S=1/M=2/L=3 heuristic, not duration.',
                'host_policy': 'All running/review blocks count as local; measurement_host excludes all others.'}

    def packet(self, bid, preview=False, base=None, worktree=None, branch=None, attempt=None, repo=None):
        require(bid in self.blocks, f'unknown block: {bid}')
        reasons = self.reasons(bid)
        if not preview:
            require(not reasons, f"{bid} is not ready: {'; '.join(reasons)}")
            assignment(dict(attempt=attempt, worktree=worktree, branch=branch, base_commit=base), bid)
            require(attempt > self.state['blocks'][bid]['attempt'], 'attempt must exceed previous attempt')
            require(branch != self.spec['integration_branch'], 'worker cannot use integration branch')
            for active in self.active():
                e = self.state['blocks'][active]
                require(Path(worktree).resolve() != Path(e['worktree']).resolve() and branch != e['branch'], 'assignment conflicts with active worker')
            def git(*args):
                return subprocess.run(['git', '-C', str(repo or HERE.parents[2]), *args], capture_output=True, text=True, check=False)
            require(git('cat-file', '-t', base).stdout.strip() == 'commit', 'base is not an existing Git commit')
            require(git('cat-file', '-e', f'{base}:{PLAN}blocks.json').returncode == 0, 'base lacks committed successor plan blocks.json; checkpoint alone cannot dispatch')
            committed_plan = git('show', f'{base}:{PLAN}blocks.json')
            require(committed_plan.returncode == 0, 'cannot read committed successor plan')
            require(json.loads(committed_plan.stdout) == self.spec,
                    'base has stale blocks.json contract; commit the current plan before dispatch')
            required = {self.spec['checkpoint_commit'], self.state['integration_head']} | {
                self.state['blocks'][d]['integration_commit'] for d in self.ancestors(bid)}
            for commit in required:
                require(git('merge-base', '--is-ancestor', commit, base).returncode == 0, f'base must contain accepted checkpoint/integration/dependency commit {commit}')
        b = self.blocks[bid]
        lines = [f"# {bid}: {b['title']}", '', LEAF, '', f"Goal: {b['goal']}", '',
                 f"Mode: {'PREVIEW ONLY — NOT FOR DISPATCH' if preview else 'Ready worker packet'}",
                 f"Base: {base or 'UNASSIGNED'}", f"Worktree: {worktree or 'UNASSIGNED'}", f"Branch: {branch or 'UNASSIGNED'}",
                 f"Attempt: {attempt or 'UNASSIGNED'}", f"Immutable checkpoint: {self.spec['checkpoint_commit']}", '',
                 '## Required reads', '', 'Call MegaMemory overview/query and read Serena instructions before coding.',
                 'Read applicable AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md,',
                 f"{self.spec['source_plan']}, {self.spec['historical_ledger']},",
                 f'{PLAN}README.md, {PLAN}blocks.json, {PLAN}state.json and {PLAN}result-template.json.',
                 'Read the accepted predecessor reports/contracts at the exact integration commits below; verify their hashes and availability before coding.',
                 'This block entry and packet are the assigned plan.', '', '## Dependency and gate receipts', '']
        for dep in b['depends_on']:
            e = self.state['blocks'][dep]
            lines.append(f"- {dep}: {e['status']}; integration={e.get('integration_commit')}; receipt={e.get('integration_receipt')}")
        for gid in b['gates']:
            lines.append(f"- {gid}: {json.dumps(self.state['gates'][gid])}; {self.spec['gates'][gid]['meaning']}")
        lines += ['', 'Unfulfilled readiness: ' + ('; '.join(reasons) or 'none'), '']
        sections = [('Owned paths', b['owns']), ('Reserved resources', b['resource_locks']),
                    ('Non-goals', self.spec['global_non_goals'] + b['non_goals']), ('Consumed contracts', b['consumes']),
                    ('Produced contracts', b['produces']), ('Steps', b['steps']), ('Acceptance', b['acceptance']), ('Verification commands', b['verification'])]
        for title, items in sections:
            lines += [f'## {title}', ''] + [('- [ ] ' if title == 'Steps' else '- ') + item for item in items] + ['']
        lines += ['## Evidence and handoff', '',
                  'Preserve immutable checkpoint evidence. Candidate checks identify actual source/base SHAs, tool/environment hashes and fresh command logs.',
                  'Conditional body lemmas/readiness smokes do not establish full runtime proof, source acceptance, package integration or comparative superiority.',
                  'All running/review blocks count as local. measurement_host excludes every other active block.',
                  'Do not edit another owner, the board/state, root Cargo manifests or shared design documents. Return concrete blockers/change requests for work outside ownership.',
                  'Commit only the bounded deliverable in your assigned worktree and stop for coordinator review. Do not merge, push, create chats/worktrees, delegate or start a successor.',
                  f'Write a structured result.json using {PLAN}result-template.json and the receipt contract in {PLAN}README.md.',
                  'Include dependency/source/tool identities, command/cwd/expected and actual exit/status/log, artifacts, claims, blockers and decisions_used.',
                  'Worker receipt status vocabulary: DONE, DONE_WITH_CONCERNS, NEEDS_CONTEXT, BLOCKED.',
                  '```json', json.dumps({'block_id': bid, 'attempt': attempt, 'status': 'NEEDS_CONTEXT',
                                        'base_commit': base, 'dependency_commits': {d: self.state['blocks'][d]['integration_commit'] for d in b['depends_on']},
                                        'scope': b['goal'], 'changed_paths': [], 'artifacts': [], 'checks': [], 'claims': [],
                                        'blockers': [], 'decisions_used': []}, indent=2), '```',
                  'A pass requires actual executed evidence; preserve fail/blocked/not_run and expected-negative scope. Never mark yourself integrated.',
                  'Return changed files, checks, limitations and the exact final 40-character worker commit SHA separately; committed result.json cannot contain its own commit hash.',
                  'Coordinator acceptance and combined-tree checks are separate from worker completion. Packet generation only prints instructions.']
        return '\n'.join(lines) + '\n'


def load(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f'duplicate JSON key: {key}')
            result[key] = value
        return result
    return json.loads(path.read_text(), object_pairs_hook=unique)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--blocks', type=Path, default=HERE / 'blocks.json')
    parser.add_argument('--state', type=Path, default=HERE / 'state.json')
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('check')
    sub.add_parser('ready')
    packet = sub.add_parser('packet')
    packet.add_argument('id')
    packet.add_argument('--preview', action='store_true')
    packet.add_argument('--base')
    packet.add_argument('--worktree')
    packet.add_argument('--branch')
    packet.add_argument('--attempt', type=int)
    args = parser.parse_args(argv)
    try:
        board = Board(load(args.blocks), load(args.state))
        if args.command == 'packet':
            print(board.packet(args.id, args.preview, args.base, args.worktree, args.branch, args.attempt), end='')
        elif args.command == 'ready':
            print(json.dumps(board.ready(), indent=2))
        else:
            print(f'OK: {len(board.blocks)} blocks; schema, DAG, gates, ownership and state valid. No work dispatched.')
        return 0
    except (ValueError, OSError, TypeError, KeyError) as error:
        print(f'board: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
