#!/usr/bin/env python3
"""Check the independent validator cannot accept missing or invalid counters."""
import unittest
from run import roots_ok


class RootEvidence(unittest.TestCase):
    def test_complete_balanced_observation(self):
        self.assertTrue(roots_ok(self.valid()))

    @staticmethod
    def valid():
        return dict(baseline_roots=0, final_roots=0, roots_registered=3,
                    roots_released=3, cleanup_failures=0, guard_entries=2,
                    guard_exits=2, recovered=1, charged_bytes=455)

    def test_every_field_is_required_and_a_nonnegative_integer(self):
        for field in self.valid():
            missing = self.valid()
            del missing[field]
            self.assertFalse(roots_ok(missing), field)
            for bad in (None, False, True, -1, '0', 0.0):
                self.assertFalse(roots_ok(dict(self.valid(), **{field: bad})), (field, bad))

    def test_unbalanced_observations_fail(self):
        for field, value in [('final_roots', 1), ('baseline_roots', 1),
                             ('roots_released', 2), ('cleanup_failures', 1),
                             ('guard_exits', 1)]:
            self.assertFalse(roots_ok(dict(self.valid(), **{field: value})))


if __name__ == '__main__':
    unittest.main()
