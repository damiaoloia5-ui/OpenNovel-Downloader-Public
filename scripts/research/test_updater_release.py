import base64
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('updater_release', ROOT / 'scripts/updater_release.py')
updater = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(updater)


def signature(version):
    # Original syntactic fixture, not a real cryptographic signature.
    return base64.b64encode(f'untrusted comment: fixture\nfixture\ntrusted comment: timestamp:123\tfile:fixture.exe\tversion:{version}\nfixture'.encode()).decode()


class UpdaterReleaseTests(unittest.TestCase):
    def test_exact_version_binding_and_public_project_url(self):
        result = updater.manifest('1.2.3', signature('1.2.3'))
        self.assertEqual(result['platforms']['windows-x86_64']['url'],
                         updater.RELEASES + '/v1.2.3/OpenNovel-1.2.3-Windows-x64-setup.exe')
        for value in ['1.2.30', '1.2.3-other', '2.0.0']:
            with self.assertRaises(ValueError):
                updater.manifest('1.2.3', signature(value))
        for value in ['../1.2.3', '1.2.3-beta', '1.2.3+unsafe']:
            with self.assertRaises(ValueError):
                updater.manifest(value, signature(value))

    def test_invalid_encoding_is_rejected(self):
        for value in ['', 'not base64!', base64.b64encode(b'no version').decode()]:
            with self.assertRaises(ValueError):
                updater.manifest('1.2.3', value)

    def test_privacy_receipt_is_required_before_reading_private_key(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            installer = root / 'fixture.exe'
            installer.write_bytes(b'MZ original installer fixture')
            privacy = root / 'privacy.json'
            privacy.write_text(json.dumps({'passed': True, 'private_library_included': True}), encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'privacy receipt'):
                updater.prepare(installer, privacy, root / 'out', root / 'absent.exe')
            self.assertFalse((root / 'out').exists())

    def test_compiled_configuration_preserves_data_identity_and_restricts_endpoint(self):
        config = json.loads((ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        self.assertEqual(config['identifier'], 'org.opennovel.downloader')
        self.assertEqual(config['bundle']['windows']['nsis']['installMode'], 'currentUser')
        self.assertTrue(config['plugins']['updater']['requireSignedVersion'])
        self.assertFalse(config['plugins']['updater']['allowDowngrades'])
        self.assertEqual(config['plugins']['updater']['endpoints'], [updater.RELEASES.removesuffix('/download') + '/latest/download/latest.json'])
        self.assertEqual(config['plugins']['updater']['pubkey'], updater.PUBLIC_KEY.read_text().strip())
        for capability in (ROOT / 'apps/desktop/src-tauri/capabilities').glob('*.json'):
            self.assertNotIn('updater:', capability.read_text())

    @unittest.skipUnless(os.name == 'nt', 'Windows DPAPI only')
    def test_dpapi_roundtrip_does_not_store_plaintext(self):
        value = b'original test signing secret - not an actual key'
        encrypted = updater.protect(value)
        self.assertNotIn(value, encrypted)
        self.assertEqual(updater.protect(encrypted, decrypt=True), value)


if __name__ == '__main__':
    unittest.main()
