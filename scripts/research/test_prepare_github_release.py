import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location('prepare_github_release', Path(__file__).resolve().parents[1] / 'prepare_github_release.py')
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.delivery = self.root / 'delivery'
        self.delivery.mkdir()
        self.output = self.root / 'v0.3.19'
        self.installer = 'OpenNovel Downloader_0.3.19_x64-setup.exe'
        self.portable = 'OpenNovel-0.3.19-Windows.zip'
        (self.delivery / self.installer).write_bytes(b'original installer fixture')
        self.source = b'original source fixture'
        with zipfile.ZipFile(self.delivery / self.portable, 'w') as archive:
            prefix = 'OpenNovel-0.3.19-Windows/'
            archive.writestr(prefix + 'opennovel-engine/native-engine/corresponding-source.tar.gz', self.source)
            archive.writestr(prefix + 'LICENSE', 'original license fixture')
            archive.writestr(prefix + 'THIRD_PARTY_NOTICES.txt', 'original notices fixture')
        self.receipt = {'passed': True, 'private_library_included': False, 'files': [
            {'name': name, 'bytes': (self.delivery / name).stat().st_size, 'sha256': release.digest(self.delivery / name)}
            for name in (self.installer, self.portable)]}
        self.manifest = {'version': '0.3.19', 'source_commit': 'a' * 40, 'installer': self.installer,
                         'installer_sha256': release.digest(self.delivery / self.installer)}
        self.lock = {'files': {'corresponding-source.tar.gz': {'bytes': len(self.source), 'sha256': hashlib.sha256(self.source).hexdigest()}}}
        self.write_receipts()
        self.git_patch = patch.object(release, 'git', side_effect=self.fake_git)
        self.git_patch.start()
        self.addCleanup(self.git_patch.stop)

    def write_receipts(self):
        for name, data in [('release-manifest.json', self.manifest), ('delivery-check.json', self.receipt)]:
            (self.delivery / name).write_text(json.dumps(data), encoding='utf-8')

    def fake_git(self, repo, *args):
        if args[0] == 'show':
            return json.dumps({'version': '0.3.19'} if args[1].endswith(':package.json') else self.lock).encode()
        self.assertEqual(args[0], 'archive')
        destination = next(a.removeprefix('--output=') for a in args if a.startswith('--output='))
        with zipfile.ZipFile(destination, 'w') as archive:
            archive.writestr('OpenNovel-0.3.19/README.md', 'original source fixture')
        return b''

    def test_archive_excludes_unlisted_private_files_and_checks_every_asset(self):
        (self.delivery / 'library.db').write_bytes(b'private fixture')
        (self.delivery / 'session.log').write_bytes(b'private fixture')
        result = release.prepare(self.root, self.delivery, self.output)
        self.assertFalse(result['ready_for_stable_release'])
        self.assertEqual(result['source_commit'], 'a' * 40)
        self.assertFalse((self.output / 'library.db').exists())
        self.assertFalse((self.output / 'session.log').exists())
        lines = (self.output / 'SHA256SUMS.txt').read_text().splitlines()
        self.assertEqual(len(lines), len(list(self.output.iterdir())) - 1)
        for line in lines:
            expected, name = line.split('  ', 1)
            self.assertEqual(expected, release.digest(self.output / name))

    def test_old_release_is_never_overwritten(self):
        self.output.mkdir()
        sentinel = self.output / 'keep.txt'
        sentinel.write_text('keep')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            release.prepare(self.root, self.delivery, self.output)
        self.assertEqual(sentinel.read_text(), 'keep')

    def test_modified_installer_is_rejected_before_output(self):
        (self.delivery / self.installer).write_bytes(b'tampered fixture')
        with self.assertRaisesRegex(ValueError, 'artifact changed'):
            release.prepare(self.root, self.delivery, self.output)
        self.assertFalse(self.output.exists())

    def test_unlisted_artifact_is_rejected(self):
        self.receipt['files'].append({'name': '../library.db'})
        self.write_receipts()
        with self.assertRaisesRegex(ValueError, 'exactly'):
            release.prepare(self.root, self.delivery, self.output)

    def test_wrong_version_is_rejected(self):
        self.manifest['version'] = '0.3.20'
        self.write_receipts()
        with self.assertRaisesRegex(ValueError, 'source version'):
            release.prepare(self.root, self.delivery, self.output)

    def test_mismatched_corresponding_source_is_rejected(self):
        self.lock['files']['corresponding-source.tar.gz']['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'corresponding source'):
            release.prepare(self.root, self.delivery, self.output)
        self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
