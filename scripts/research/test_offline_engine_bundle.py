"""Original tiny archives and mocked Docker; no image loads or network requests in normal CI."""
import hashlib
import json
import pathlib
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

import offline_engine_bundle as bundle
import desktop_engine_worker as worker


class OfflineBundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.directory = self.root / 'offline-engine'
        self.directory.mkdir()
        self.lock = {'schema_version': 1, 'image_id': 'sha256:' + '1' * 64,
                     'binary_path': '/opt/opennovel/opennovel_engine_service', 'files': {}}
        for name in bundle.FILES:
            data = ('Original archive fixture ' + name).encode()
            (self.directory / name).write_bytes(data)
            self.lock['files'][name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        self.raw = json.dumps(self.lock).encode()
        (self.directory / 'manifest.json').write_bytes(self.raw)
        self.addCleanup(patch.stopall)
        patch.object(bundle.sandbox, 'PREVIOUS', self.root).start()
        patch.object(bundle, '_lock', return_value=(self.raw, self.lock)).start()

    def test_manifest_cannot_override_the_compiled_lock_or_changed_archives(self):
        self.assertEqual(bundle.verify(), self.lock)
        (self.directory / 'manifest.json').write_bytes(b'{}')
        with self.assertRaises(bundle.BundleError):
            bundle.verify()
        (self.directory / 'manifest.json').write_bytes(self.raw)
        (self.directory / 'runtime-image.tar.gz').write_bytes(b'altered')
        with patch.object(bundle.subprocess, 'run') as run:
            with self.assertRaises(bundle.BundleError):
                bundle.install()
            run.assert_not_called()

    def test_extra_files_and_missing_corresponding_source_are_refused(self):
        (self.directory / 'unexpected.txt').write_text('original obstacle')
        with self.assertRaises(bundle.BundleError):
            bundle.verify()
        (self.directory / 'unexpected.txt').unlink()
        (self.directory / 'corresponding-source.tar.gz').unlink()
        with self.assertRaises(bundle.BundleError):
            bundle.verify()

    def test_ready_image_is_reused_without_load_or_install(self):
        with patch.object(bundle, 'image_ready', return_value=True), patch.object(bundle.subprocess, 'run') as run:
            self.assertEqual(bundle.install(), self.lock)
            run.assert_not_called()

    def test_first_prepare_loads_only_the_verified_local_archive_and_checks_identity(self):
        with patch.object(bundle, 'image_ready', side_effect=[False, True]), patch.object(bundle.subprocess, 'run') as run:
            bundle.install()
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0], [bundle.sandbox.DOCKER, 'image', 'load', '--quiet', '--input', str(self.directory / 'runtime-image.tar.gz')])
            self.assertEqual(run.call_args.kwargs['timeout'], 120)

    def test_failed_import_preserves_archives_and_has_no_fallback_or_prune(self):
        before = (self.directory / 'runtime-image.tar.gz').read_bytes()
        with patch.object(bundle, 'image_ready', return_value=False), patch.object(bundle.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, 'original-fixture')) as run:
            with self.assertRaises(bundle.BundleError) as raised:
                bundle.install()
            self.assertEqual(raised.exception.reason, 'offline_import_failed')
            self.assertEqual(run.call_count, 1)
        self.assertEqual((self.directory / 'runtime-image.tar.gz').read_bytes(), before)

    def test_image_metadata_must_match_fixed_identity_platform_and_nonroot_user(self):
        image = {'Id': self.lock['image_id'], 'Os': 'linux', 'Architecture': 'amd64', 'Config': {'User': '10001:10001'}}
        with patch.object(bundle.subprocess, 'run', return_value=types.SimpleNamespace(stdout=json.dumps([image]).encode())):
            self.assertTrue(bundle.image_ready(self.lock))
        image['Config']['User'] = 'root'
        with patch.object(bundle.subprocess, 'run', return_value=types.SimpleNamespace(stdout=json.dumps([image]).encode())):
            with self.assertRaises(bundle.BundleError):
                bundle.image_ready(self.lock)

    def test_readonly_check_reports_pending_and_only_explicit_prepare_imports(self):
        with patch.object(worker.sandbox, 'verify_source'), patch.object(worker.subprocess, 'run', return_value=types.SimpleNamespace(stdout='linux')) as run, patch.object(bundle, 'image_ready', return_value=False), patch.object(bundle, 'install') as install:
            self.assertEqual(worker.check(), 'offline_bundle_pending')
            install.assert_not_called()
            self.assertEqual(run.call_count, 1)
        with patch.object(worker, 'check', side_effect=['offline_bundle_pending', None]), patch.object(bundle, 'install') as install:
            self.assertIsNone(worker.prepare())
            install.assert_called_once()


if __name__ == '__main__':
    unittest.main()
