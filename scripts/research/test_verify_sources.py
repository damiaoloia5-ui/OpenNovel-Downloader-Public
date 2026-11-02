"""Offline checks of live-check orchestration; never contact novel platforms."""
import importlib.util
from contextlib import closing
import json
import pathlib
import sqlite3
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location('verify_sources', pathlib.Path(__file__).resolve().parents[1] / 'verify_sources.py')
checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checks)


class CompatibilityChecks(unittest.TestCase):
    def test_bounded_sampling_preserves_order_and_restriction(self):
        chapters = [{'restricted': i > 9} for i in range(1000)]
        self.assertEqual(checks.select_chapters(chapters), [1, 6, 10, 11])
        self.assertEqual(checks.select_chapters([{'restricted': False}]), [1])
        self.assertEqual(checks.select_chapters([{'restricted': True}] * 5), [1])
        self.assertEqual(checks.select_chapters([]), [])

    def test_expected_partial_is_not_a_failure_but_wrong_counts_are(self):
        catalog = {'chapters': [{'restricted': False}, {'restricted': True}]}
        job = {'saved': 1, 'blocked': 1, 'failed': 0, 'total': 2}
        self.assertEqual(checks.validate_job(2, job, catalog, [1, 2]), 1)
        for wrong in [dict(job, failed=1), dict(job, saved=2), dict(job, blocked=0), dict(job, total=3)]:
            with self.assertRaises(ValueError):
                checks.validate_job(2, wrong, catalog, [1, 2])
        with self.assertRaises(ValueError):
            checks.validate_job(0, job, catalog, [1, 2])

    def test_failure_stops_further_requests_for_that_source(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(checks, 'run_cli', return_value=(1, None)) as command:
            rows = checks.check_source('qimao-web', ['controlled:first', 'controlled:second'], pathlib.Path('unused'), pathlib.Path(tmp))
            self.assertEqual(command.call_count, 1)
            self.assertEqual([r['passed'] for r in rows], [False, False])

    def test_http_200_verification_is_not_body_support(self):
        self.assertIn('verification', checks.entry_status(200, 'https://www.ciweimao.com/signup/man_machine_verify'))
        self.assertIn('not proven', checks.entry_status(200, 'https://b.faloo.com/123.html'))
        self.assertIn('verification', checks.entry_status(202, 'https://www.qidian.com/book/123/'))
        self.assertEqual(checks.entry_status(500, 'https://www.qidian.com/'), 'unexpected-http-status')

    def test_redirect_cannot_leave_official_https_hosts(self):
        redirects = checks.OfficialRedirects({'www.ciweimao.com'})
        for url in ['http://www.ciweimao.com/', 'https://other.test/', 'https://user:secret@www.ciweimao.com/', 'https://www.ciweimao.com:8443/']:
            with self.assertRaises(ValueError):
                redirects.redirect_request(None, None, 302, '', {}, url)

    def test_txt_requires_actual_saved_paragraphs_and_correct_coverage(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / 'exports').mkdir()
            path = root / 'exports/original.txt'
            path.write_text('原创山谷。\n原创河岸。', encoding='utf-8')
            with closing(sqlite3.connect(root / 'library.db')) as db:
                db.execute('CREATE TABLE chapters(paragraphs TEXT)')
                db.execute('INSERT INTO chapters VALUES (?)', (json.dumps(['原创山谷。', '原创河岸。']),))
                db.commit()
            receipt = {'path': str(path), 'saved_chapters': 1, 'total_chapters': 2, 'partial': True}
            self.assertGreater(checks.verify_export(receipt, root, 1, 2, 'txt')['bytes'], 0)
            path.write_text('原创山谷。', encoding='utf-8')
            with self.assertRaises(ValueError):
                checks.verify_export(receipt, root, 1, 2, 'txt')
            with self.assertRaises(ValueError):
                checks.verify_export(dict(receipt, partial=False), root, 1, 2, 'txt')
            with self.assertRaises(ValueError):
                checks.verify_export(dict(receipt, path=str(root / 'outside.txt')), root, 1, 2, 'txt')


if __name__ == '__main__':
    unittest.main()
