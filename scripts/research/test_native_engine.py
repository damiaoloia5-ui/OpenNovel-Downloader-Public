"""Original trust-boundary fixtures. Ordinary CI never launches the engine or a platform."""
import hashlib
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import native_engine as native
import desktop_engine_worker as worker


class NativeBoundaryTests(unittest.TestCase):
    def test_native_failure_never_falls_back_to_docker_or_installation(self):
        for reason in ('native_bundle_invalid', 'native_sandbox_failed', None):
            with patch.object(worker.sandbox, 'verify_source'), patch.object(native, 'present', return_value=True), patch.object(native, 'check', return_value=reason), patch.object(worker.subprocess, 'run') as command:
                self.assertEqual(worker.prepare(), reason)
                command.assert_not_called()

    def test_invalid_id_or_offline_without_fixture_never_starts_process(self):
        with patch.object(native, 'IsolatedProcess') as process:
            for book, items in [('..', ['10000000002']), ('10000000001', []), ('10000000001', ['https://example.invalid'])]:
                with self.assertRaises(ValueError):
                    native.NativeSession(book, items, fixture=lambda _: {})
            with self.assertRaises(ValueError):
                native.NativeSession('10000000001', ['10000000002'])
            process.assert_not_called()

    def test_binary_manifest_and_source_archive_all_require_fixed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            directory = root / 'native-engine'; directory.mkdir()
            scripts = root / 'scripts'; scripts.mkdir()
            files = {}
            for name in ['engine.exe', 'corresponding-source.tar.gz']:
                data = ('original fixture ' + name).encode()
                (directory / name).write_bytes(data)
                files[name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
            raw = json.dumps({'files': files}).encode()
            (scripts / 'native_engine_lock.json').write_bytes(raw)
            (directory / 'manifest.json').write_bytes(raw)
            with patch.object(native.config, 'PREVIOUS', root), patch.object(native, '__file__', str(scripts / 'native_engine.py')):
                self.assertEqual(native.verify()[0], directory / 'engine.exe')
                original = (directory / 'engine.exe').read_bytes()
                (directory / 'engine.exe').write_bytes(b'x' * len(original))
                with self.assertRaises(ValueError): native.verify()
                (directory / 'engine.exe').write_bytes(original)
                (directory / 'unexpected.dll').write_bytes(b'not allowed')
                with self.assertRaises(ValueError): native.verify()
                (directory / 'unexpected.dll').unlink()
                (directory / 'manifest.json').write_bytes(b'{}')
                with self.assertRaises(ValueError): native.verify()


if __name__ == '__main__':
    unittest.main()
