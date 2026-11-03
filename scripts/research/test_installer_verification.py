"""Safety gates and data-loss detection; these tests never install software."""
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_installer as verifier


class InstallerTests(unittest.TestCase):
    def test_non_hosted_run_refuses_before_registry_or_process_access(self):
        for env in [{}, {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'self-hosted', 'RUNNER_OS': 'Windows'}]:
            with self.subTest(env=env), patch.dict(os.environ, env, clear=True), \
                    patch.object(verifier, 'registered_locations') as registry, patch.object(verifier, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'disposable'):
                    verifier.main()
                registry.assert_not_called()
                run.assert_not_called()

    def test_runner_identity_is_explicit_and_windows_only(self):
        env = {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted', 'RUNNER_OS': 'Windows'}
        verifier.require_hosted_runner(env, 'win32')
        with self.assertRaises(ValueError):
            verifier.require_hosted_runner(env, 'linux')
        with self.assertRaises(ValueError):
            verifier.require_hosted_runner({**env, 'RUNNER_OS': 'Linux'}, 'win32')

    def test_existing_registration_refuses_before_installation(self):
        repo = Path(verifier.__file__).resolve().parents[1]
        with patch.object(verifier, 'require_hosted_runner'), \
                patch.dict(os.environ, {'GITHUB_WORKSPACE': str(repo)}), \
                patch.object(verifier, 'registered_locations', return_value=['existing']), \
                patch.object(verifier, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'Existing OpenNovel registration'):
                verifier.main()
            run.assert_not_called()

    def test_destinations_reject_root_and_parent_escape(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.assertEqual(verifier.contained(root / 'Installed App', root), root.resolve() / 'Installed App')
            for path in [root, root / '../outside', root.parent]:
                with self.subTest(path=path), self.assertRaises(ValueError):
                    verifier.contained(path, root)

    def test_nsis_destination_is_last_unquoted_without_shell_interpolation(self):
        command = verifier.nsis_command(Path('C:/original installer.exe'), ['/S', '/NS'], '/D=C:/Original Test/Installed App')
        self.assertTrue(command.endswith(' /D=C:/Original Test/Installed App'))
        for bad in ['/D=C:/a"b', '/D=C:/a\nb', '/D=C:/a\0b']:
            with self.assertRaises(ValueError):
                verifier.nsis_command(Path('installer.exe'), ['/S'], bad)

    def test_preservation_detects_modification_and_deletion(self):
        with tempfile.TemporaryDirectory() as name:
            path = Path(name) / 'original.txt'
            path.write_text('Original acceptance fixture', encoding='utf-8')
            expected = {path: verifier.sha256(path)}
            verifier.verify_preserved(expected)
            path.write_text('changed', encoding='utf-8')
            with self.assertRaises(ValueError):
                verifier.verify_preserved(expected)
            path.unlink()
            with self.assertRaises(ValueError):
                verifier.verify_preserved(expected)


if __name__ == '__main__':
    unittest.main()
