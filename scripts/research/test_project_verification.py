"""Controlled verification-runner contracts; no actual build or native window."""
import json
import pathlib
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import verify_project as verifier


class VerificationTests(unittest.TestCase):
    def test_integrated_steps_require_explicit_input_and_finish_with_native_check(self):
        root = pathlib.Path('D:/original')
        with self.assertRaises(ValueError):
            verifier.build_steps(root, 'integrated', True, True)
        steps = verifier.build_steps(root, 'integrated', True, True, pathlib.Path('D:/bundle'))
        self.assertIn('--offline', dict(steps)['rust-tests'])
        self.assertIn('--locked', dict(steps)['rust-tests'])
        self.assertEqual(steps[-1][0], 'native-installation')
        self.assertLess([s[0] for s in steps].index('integrated-resources'), [s[0] for s in steps].index('desktop-build'))
        self.assertEqual(dict(steps)['desktop-build'][-1], 'desktop:package:integrated')
        self.assertNotIn('integrated-resources', dict(verifier.build_steps(root, 'basic', False, False)))

    def test_native_gate_rejects_stale_unverified_and_out_of_scope_evidence(self):
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            with self.assertRaises(ValueError):
                verifier.inspect_native(root, '1.2.3')
            path = root / 'native/check-original/report.json'
            path.parent.mkdir(parents=True)
            good = {'version': '1.2.3', 'required_checks_passed': True, 'deleted_sample_txt_recovered': True,
                    'user_library_opened': False, 'real_platform_requested': False, 'native_window_interaction_checked': False}
            for key, value in [('version', 'old'), ('required_checks_passed', False), ('deleted_sample_txt_recovered', False),
                               ('user_library_opened', True), ('real_platform_requested', True), ('native_window_interaction_checked', True)]:
                path.write_text(json.dumps({**good, key: value}), encoding='utf-8')
                with self.subTest(key=key), self.assertRaises(ValueError):
                    verifier.inspect_native(root, '1.2.3')
            path.write_text(json.dumps(good), encoding='utf-8')
            self.assertTrue(verifier.inspect_native(root, '1.2.3').endswith('report.json'))

    def test_failed_command_and_missing_tool_never_become_passed(self):
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            with patch.object(verifier.shutil, 'which', return_value='original-tool'), patch.object(verifier.subprocess, 'call', return_value=7):
                result = verifier.run_step('original', ['tool'], root, root, {})
                self.assertFalse(result['passed'])
                self.assertEqual(result['exit_code'], 7)
            with patch.object(verifier.shutil, 'which', return_value=None), patch.object(verifier.subprocess, 'call', side_effect=FileNotFoundError('missing')):
                self.assertFalse(verifier.run_step('missing', ['tool'], root, root, {})['passed'])

    def test_runtime_minimum_is_checked_before_building(self):
        with patch.object(verifier.subprocess, 'check_output', side_effect=['v22.12.0', 'rustc 1.96.0']), patch.object(verifier.shutil, 'which', return_value='tool'):
            self.assertEqual(verifier.check_runtime()['node'], '22.12.0')
        with patch.object(verifier.subprocess, 'check_output', return_value='v20.0.0'):
            with self.assertRaises(ValueError):
                verifier.check_runtime()

    def test_source_archives_report_unknown_git_metadata_without_initializing(self):
        with patch.object(verifier.subprocess, 'run', return_value=type('Result', (), {'returncode': 1})()) as run:
            result = verifier.source_context(pathlib.Path('original-source'))
            self.assertIsNone(result['source_commit'])
            self.assertIsNone(result['source_dirty'])
            self.assertEqual(result['source_provenance'], 'git-metadata-unavailable')
            run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
