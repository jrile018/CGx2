"""Negative scheduling/dispatch controls, plus local Git ancestry preflights."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
MODULE = HERE / 'board.py'
spec = importlib.util.spec_from_file_location('board', MODULE)
board = importlib.util.module_from_spec(spec)
spec.loader.exec_module(board)
DATA = HERE


class BoardTests(unittest.TestCase):
    def setUp(self):
        self.plan = board.load(DATA / 'blocks.json')
        self.state = board.load(DATA / 'state.json')
        self.sha = self.plan['checkpoint_commit']

    def b(self, bid):
        return next(b for b in self.plan['blocks'] if b['id'] == bid)

    def make(self):
        return board.Board(self.plan, self.state)

    def integrated(self, bid, sha=None):
        self.state['blocks'][bid].update(status='integrated', integration_commit=sha or self.sha,
                                          integration_receipt=f'research/architecture-review/next/integration/{bid}.json')

    def active(self, bid, status='running'):
        self.state['blocks'][bid].update(status=status, attempt=1, worker_kind='chat', worker_id=bid,
                                         worktree=f'/tmp/worker-{bid}', branch=f'riley/{bid.lower()}', base_commit=self.sha,
                                         worker_commit=self.sha if status == 'review' else None,
                                         receipt=f'next/{bid}/result.json' if status == 'review' else None)

    def approve(self, gid):
        self.state['gates'][gid].update(status='approved', decision_ref='decisions/scope.md')

    def test_canonical_board_and_weighted_shortlist(self):
        b = self.make()
        result = b.ready()
        self.assertEqual({r['id'] for r in result['eligible']}, {'P0', 'O0', 'F0', 'E0', 'E1'})
        self.assertLessEqual(len(result['dispatch_shortlist']), 3)
        weights = [r['remaining_weight'] for r in result['eligible']]
        self.assertEqual(weights, sorted(weights, reverse=True))
        self.assertEqual(b.critical_path('Z0'), 3)

    def test_cycle_rejected(self):
        self.b('P0')['depends_on'] = ['P1']
        with self.assertRaisesRegex(ValueError, 'cycle'):
            self.make()

    def test_missing_dependency_rejected(self):
        self.b('P0')['depends_on'] = ['MISSING']
        with self.assertRaisesRegex(ValueError, 'missing dependency'):
            self.make()

    def test_all_block_state_ids_required(self):
        del self.state['blocks']['P0']
        with self.assertRaisesRegex(ValueError, 'match spec'):
            self.make()

    def test_revised_graph_block_count_is_allowed(self):
        self.plan['blocks'] = [b for b in self.plan['blocks'] if b['id'] != 'Z0']
        del self.state['blocks']['Z0']
        self.assertEqual(len(self.make().blocks), 19)

    def test_empty_graph_rejected(self):
        self.plan['blocks'] = []
        self.state['blocks'] = {}
        with self.assertRaisesRegex(ValueError, 'nonempty blocks'):
            self.make()

    def test_missing_contract_rejected(self):
        self.b('P0')['acceptance'] = []
        with self.assertRaisesRegex(ValueError, 'acceptance'):
            self.make()

    def test_unknown_gate_rejected(self):
        self.b('P0')['gates'] = ['missing']
        with self.assertRaisesRegex(ValueError, 'unknown gate'):
            self.make()

    def test_withheld_gate(self):
        self.integrated('P0')
        self.state['gates']['proof_scope']['status'] = 'withheld'
        b = self.make()
        self.assertNotIn('P1', [r['id'] for r in b.ready()['eligible']])
        self.assertIn('withheld', b.packet('P1', preview=True))

    def test_approved_gate_requires_decision(self):
        self.state['gates']['proof_scope']['status'] = 'approved'
        with self.assertRaisesRegex(ValueError, 'decision_ref'):
            self.make()

    def test_dependency_not_integrated(self):
        self.active('P0', 'review')
        self.approve('proof_scope')
        self.assertTrue(any('dependency P0: review' in r for r in self.make().reasons('P1')))

    def test_integrated_requires_receipt_and_sha(self):
        for field in ('integration_receipt', 'integration_commit'):
            with self.subTest(field=field):
                self.integrated('P0')
                self.state['blocks']['P0'][field] = None
                with self.assertRaisesRegex(ValueError, 'integration SHA and receipt'):
                    self.make()

    def test_unordered_shared_owner_rejected(self):
        self.b('O0')['owns'].append('research/architecture-review/next/P0/nested/')
        with self.assertRaisesRegex(ValueError, 'unordered shared ownership'):
            self.make()

    def test_component_prefixes_not_string_prefixes(self):
        self.assertFalse(board.overlaps('crates/abc/', 'crates/abcd/'))
        self.assertTrue(board.overlaps('crates/abc/', 'crates/abc/src/'))

    def test_review_reserves_locks(self):
        self.active('P0', 'review')
        self.b('F0')['resource_locks'] = ['proof_toolchain']
        b = self.make()
        self.assertIn('active conflict with P0', b.reasons('F0'))

    def test_active_lock_conflict_rejected(self):
        self.active('P0')
        self.active('F0')
        self.b('F0')['resource_locks'] = ['proof_toolchain']
        with self.assertRaisesRegex(ValueError, 'active ownership/lock conflict'):
            self.make()

    def test_busy_host_blocks_measurement_and_reverse(self):
        self.b('F0')['resource_locks'] = ['measurement_host']
        self.active('O0')
        self.assertIn('active conflict with O0', self.make().reasons('F0'))
        self.state['blocks']['O0']['status'] = 'pending'
        self.active('F0')
        self.assertEqual(self.make().ready()['eligible'], [])

    def test_shortlist_avoids_pairwise_locks(self):
        self.b('O0')['resource_locks'] = ['proof_toolchain']
        result = self.make().ready()
        self.assertFalse({'P0', 'O0'} <= set(result['dispatch_shortlist']))

    def test_invalid_active_assignments(self):
        self.active('P0')
        original = copy.deepcopy(self.state['blocks']['P0'])
        for field, value in [('attempt', 0), ('worktree', 'relative'), ('branch', 'main'),
                             ('branch', 'riley/bad..ref'), ('base_commit', 'abc'), ('worker_id', ''),
                             ('worker_kind', 'unknown')]:
            with self.subTest(field=field, value=value):
                self.state['blocks']['P0'] = dict(original, **{field: value})
                with self.assertRaises(ValueError):
                    self.make()

    def test_invalid_status_and_active_dependency(self):
        self.state['blocks']['P0']['status'] = 'DONE'
        with self.assertRaisesRegex(ValueError, 'invalid state'):
            self.make()
        self.state['blocks']['P0']['status'] = 'pending'
        self.active('P1')
        with self.assertRaisesRegex(ValueError, 'unmet dependencies/gates'):
            self.make()

    def test_same_worktree_rejected(self):
        self.active('P0')
        self.active('O0')
        self.state['blocks']['O0']['worktree'] = self.state['blocks']['P0']['worktree']
        with self.assertRaisesRegex(ValueError, 'shared active assignment'):
            self.make()

    def test_packet_refuses_unready(self):
        with self.assertRaisesRegex(ValueError, 'not ready'):
            self.make().packet('P1')

    def test_packet_requires_valid_assignment(self):
        valid = dict(base=self.sha, worktree='/tmp/new-worker', branch='riley/new-worker', attempt=1)
        for field, value in [('base', 'short'), ('worktree', 'relative'), ('branch', 'main'), ('attempt', 0)]:
            with self.subTest(field=field):
                with self.assertRaises(ValueError):
                    self.make().packet('P0', **dict(valid, **{field: value}))

    def test_preview_is_complete_and_does_not_modify_state(self):
        before = copy.deepcopy(self.state)
        packet = self.make().packet('P1', preview=True)
        for required in ('PREVIEW', board.LEAF, 'proof_scope', 'dependency P0: pending',
                         'AGENTS.md', 'Consumed contracts', 'Produced contracts', '- [ ]',
                         'Verification commands', 'result-template.json', 'DONE_WITH_CONCERNS', '40-character'):
            self.assertIn(required, packet)
        self.assertEqual(self.state, before)

    def test_duplicate_json_key_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'bad.json'
            path.write_text('{"status":"pending","status":"integrated"}')
            with self.assertRaisesRegex(ValueError, 'duplicate JSON key'):
                board.load(path)

    def test_git_packet_preflight(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.run(['git', '-C', directory, *args], check=True, capture_output=True, text=True).stdout.strip()
            git('init', '-q')
            git('config', 'user.email', 'test@example.invalid')
            git('config', 'user.name', 'Board Test')
            (root / 'README').write_text('checkpoint\n')
            git('add', '.')
            git('commit', '-qm', 'checkpoint')
            checkpoint = git('rev-parse', 'HEAD')
            self.plan['checkpoint_commit'] = checkpoint
            self.state['integration_head'] = checkpoint
            kwargs = dict(base=checkpoint, worktree='/tmp/new-worker', branch='riley/test', attempt=1, repo=root)
            with self.assertRaisesRegex(ValueError, 'lacks committed successor plan'):
                self.make().packet('P0', **kwargs)
            plan_file = root / board.PLAN / 'blocks.json'
            plan_file.parent.mkdir(parents=True)
            plan_file.write_text(json.dumps(self.plan))
            git('add', '.')
            git('commit', '-qm', 'successor plan')
            plan_commit = git('rev-parse', 'HEAD')
            kwargs['base'] = plan_commit
            self.assertIn('Ready worker packet', self.make().packet('P0', **kwargs))
            with self.subTest(preflight='stale committed contract'):
                original_goal = self.b('P0')['goal']
                self.b('P0')['goal'] = original_goal + ' Changed contract.'
                with self.assertRaisesRegex(ValueError, 'stale blocks.json contract'):
                    self.make().packet('P0', **kwargs)
                self.b('P0')['goal'] = original_goal
            (root / 'README').write_text('dependency accepted\n')
            git('add', '.')
            git('commit', '-qm', 'dependency')
            dependency = git('rev-parse', 'HEAD')
            with self.subTest(preflight='integration head missing from base'):
                self.state['integration_head'] = dependency
                with self.assertRaisesRegex(ValueError, 'accepted checkpoint/integration/dependency'):
                    self.make().packet('P0', **kwargs)
                self.state['integration_head'] = checkpoint
            self.integrated('P0', dependency)
            self.approve('proof_scope')
            with self.assertRaisesRegex(ValueError, 'accepted checkpoint/integration/dependency'):
                self.make().packet('P1', **kwargs)
            kwargs['base'] = dependency
            self.assertIn(dependency, self.make().packet('P1', **kwargs))
            kwargs['base'] = '0' * 40
            with self.assertRaisesRegex(ValueError, 'existing Git commit'):
                self.make().packet('P1', **kwargs)


if __name__ == '__main__':
    unittest.main()
