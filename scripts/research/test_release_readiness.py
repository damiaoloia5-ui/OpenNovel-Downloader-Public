import datetime as dt
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_readiness as gate


class ReadinessTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / 'original.bin').write_bytes(b'original fixture, not executable')
        self.now = dt.datetime(2026, 11, 4, tzinfo=dt.timezone.utc)
        self.manifest = {'schema_version': 1, 'source_commit': 'a' * 40, 'version': 'fixture',
                         'installer': 'original.bin', 'installer_sha256': gate.digest(self.root / 'original.bin'),
                         'sources': ['local-json'], 'evidence': {}}

    def add_reports(self):
        for kind, checks in gate.REQUIRED.items():
            report = {k: self.manifest[k] for k in ['version', 'source_commit', 'installer_sha256']}
            report.update(kind=kind, passed=True, checked_at=self.now.isoformat(), evidence_mode='actual',
                          checks=dict.fromkeys(checks, True), verified_sources=['local-json'])
            self.write(kind, report)

    def write(self, kind, report):
        path = self.root / (kind + '.json')
        path.write_text(json.dumps(report), encoding='utf-8')
        self.manifest['evidence'][kind] = {'path': path.name, 'sha256': gate.digest(path)}

    def test_missing_and_mismatched_binary_are_blocked(self):
        self.assertEqual(len(gate.evaluate(self.manifest, self.root, self.now)['blockers']), 6)
        (self.root / 'original.bin').write_bytes(b'changed')
        self.assertFalse(gate.evaluate(self.manifest, self.root, self.now)['ready_for_release_review'])

    def test_fixture_consistency_does_not_publish(self):
        self.add_reports()
        report = gate.evaluate(self.manifest, self.root, self.now)
        self.assertTrue(report['ready_for_release_review'])
        self.assertFalse(report['published'])

    def test_stale_failed_different_scope_and_mock_actual_are_rejected(self):
        self.add_reports()
        for kind, field, value in [('security', 'passed', 'true'), ('engineering', 'source_commit', 'b'*40),
                                   ('signing', 'checked_at', '2026-01-01T00:00:00+00:00'),
                                   ('platforms', 'verified_sources', ['qidian-session']),
                                   ('independent_windows', 'evidence_mode', 'controlled')]:
            with self.subTest(kind=kind):
                path = self.root / (kind + '.json')
                original = gate.load(path)
                self.write(kind, {**original, field: value})
                self.assertFalse(gate.evaluate(self.manifest, self.root, self.now)['ready_for_release_review'])
                self.write(kind, original)

    def test_traversal_and_changed_evidence_are_rejected(self):
        self.add_reports()
        self.manifest['evidence']['security']['path'] = '../outside.json'
        self.assertFalse(gate.evaluate(self.manifest, self.root, self.now)['ready_for_release_review'])
        self.add_reports()
        (self.root / 'security.json').write_text('{}')
        self.assertFalse(gate.evaluate(self.manifest, self.root, self.now)['ready_for_release_review'])
