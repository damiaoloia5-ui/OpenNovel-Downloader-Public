"""Original export fixtures only; never contact the live benchmark sample URLs."""
import importlib.util
import json
import pathlib
import sys
import unittest

scripts = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(scripts))
import benchmark_books as benchmark


class BenchmarkTests(unittest.TestCase):
    def test_partial_coverage_and_order_are_checked_without_quadratic_rescans(self):
        rows = [(json.dumps(['原创第一段', '原创第二段']),)]
        receipt = {'saved_chapters': 1, 'total_chapters': 2, 'partial': True}
        self.assertTrue(benchmark.verify_txt('书名\n原创第一段\n原创第二段', rows, receipt, 2))
        self.assertFalse(benchmark.verify_txt('原创第二段\n原创第一段', rows, receipt, 2))
        self.assertFalse(benchmark.verify_txt('原创第一段', rows, receipt, 2))
        self.assertFalse(benchmark.verify_txt('原创第一段\n原创第二段', rows, dict(receipt, partial=False), 2))

    def test_duplicate_paragraphs_require_separate_occurrences(self):
        rows = [(json.dumps(['重复的原创段落', '重复的原创段落']),)]
        receipt = {'saved_chapters': 1, 'total_chapters': 1, 'partial': False}
        self.assertFalse(benchmark.verify_txt('重复的原创段落', rows, receipt, 1))
        self.assertTrue(benchmark.verify_txt('重复的原创段落\n重复的原创段落', rows, receipt, 1))

    def test_live_network_is_opt_in(self):
        from unittest.mock import patch
        with patch.object(sys, 'argv', ['benchmark_books', '--executable', 'absent.exe', '--output', 'unused']), patch.object(benchmark, 'run') as run:
            with self.assertRaises(SystemExit) as result:
                benchmark.main()
            self.assertEqual(result.exception.code, 2)
            run.assert_not_called()
