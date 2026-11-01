"""Original source-bundle fixtures only; no network, Docker or upstream code execution."""
import hashlib
import io
import json
import pathlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from reviewed_component_source import PREFIX, recover_source
import prepare_integrated_component as integrated


class ReviewedSourceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.output = self.root / 'reviewed'
        self.inventory = {'commit': 'a' * 40, 'files': [
            {'path': 'LICENSE', 'bytes': 8, 'sha256': hashlib.sha256(b'original').hexdigest()}]}

    def bundle(self, entries):
        path = self.root / 'source.tar.gz'
        with tarfile.open(path, 'w:gz') as archive:
            for name, data, kind in entries:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        return path, {'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    def test_valid_bundle_reuses_identical_source_without_copying_the_wrapper(self):
        path, lock = self.bundle([(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE),
                                  (PREFIX + 'rust/examples/opennovel_engine_service.rs', b'original wrapper', tarfile.REGTYPE)])
        source = recover_source(path, lock, self.inventory, self.output)
        self.assertEqual((source / 'LICENSE').read_bytes(), b'original')
        self.assertEqual(recover_source(path, lock, self.inventory, self.output), source)
        self.assertEqual(len(list(source.rglob('*'))), 1)

    def test_archive_tampering_is_rejected_before_creating_source(self):
        path, lock = self.bundle([(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE)])
        path.write_bytes(b'changed')
        with self.assertRaises(ValueError):
            recover_source(path, lock, self.inventory, self.output)
        self.assertFalse(self.output.exists())

    def test_unsafe_extra_duplicate_and_incomplete_entries_are_rejected(self):
        for entries in [
            [('../escape', b'original', tarfile.REGTYPE)],
            [(PREFIX + 'LICENSE', b'', tarfile.SYMTYPE)],
            [(PREFIX + 'unexpected', b'original', tarfile.REGTYPE)],
            [(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE)] * 2,
            [(PREFIX + 'LICENSE', b'changed!', tarfile.REGTYPE)],
            [],
        ]:
            with self.subTest(entries=entries):
                path, lock = self.bundle(entries)
                with self.assertRaises(ValueError):
                    recover_source(path, lock, self.inventory, self.output)
                self.assertFalse(self.output.exists())

    def test_existing_altered_source_and_inventory_are_preserved(self):
        path, lock = self.bundle([(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE)])
        source = recover_source(path, lock, self.inventory, self.output)
        (source / 'LICENSE').write_bytes(b'changed!')
        with self.assertRaises(ValueError):
            recover_source(path, lock, self.inventory, self.output)
        self.assertEqual((source / 'LICENSE').read_bytes(), b'changed!')
        (source / 'LICENSE').write_bytes(b'original')
        (self.output / 'inventory.json').write_text(json.dumps({'commit': 'other'}), encoding='utf-8')
        with self.assertRaises(ValueError):
            recover_source(path, lock, self.inventory, self.output)

    def test_relative_output_and_inventory_paths_are_refused(self):
        path, lock = self.bundle([(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE)])
        with self.assertRaises(ValueError):
            recover_source(path, lock, self.inventory, pathlib.Path('relative'))
        self.inventory['files'][0]['path'] = '../escape'
        with self.assertRaises(ValueError):
            recover_source(path, lock, self.inventory, self.output)

    def test_preparation_uses_supplied_repository_and_rejects_changed_metadata(self):
        repo = self.root / 'fresh-source'
        scripts = repo / 'scripts/research'
        scripts.mkdir(parents=True)
        (repo / 'third_party').mkdir()
        (repo / 'LICENSE').write_bytes(b'original')
        (repo / 'package.json').write_text(json.dumps({'version': '1.2.3'}), encoding='utf-8')
        for name in integrated.RUNTIME_FILES:
            (scripts / name).write_bytes(b'original runtime fixture')
        python_lock = {'files': {'python.exe': hashlib.sha256(b'original python fixture').hexdigest()}}
        (repo / 'third_party/python-embed-lock.json').write_text(json.dumps(python_lock), encoding='utf-8')
        (repo / 'third_party/local-engine-source-lock.json').write_text(json.dumps(self.inventory), encoding='utf-8')
        distribution = self.root / 'bundle'
        distribution.mkdir()
        path, source_lock = self.bundle([(PREFIX + 'LICENSE', b'original', tarfile.REGTYPE)])
        (distribution / 'corresponding-source.tar.gz').write_bytes(path.read_bytes())
        (distribution / 'runtime-image.tar.gz').write_bytes(b'original image fixture')
        engine_lock = {'candidate_commit': self.inventory['commit'], 'files': {
            'corresponding-source.tar.gz': source_lock,
            'runtime-image.tar.gz': {'bytes': 22, 'sha256': hashlib.sha256(b'original image fixture').hexdigest()}}}
        (distribution / 'manifest.json').write_text(json.dumps(engine_lock), encoding='utf-8')
        (scripts / 'engine_bundle_lock.json').write_text(json.dumps(engine_lock), encoding='utf-8')
        python_archive = self.root / 'python.zip'
        python_archive.write_bytes(b'original ZIP fixture; not executed')

        def fake_python(data, destination):
            destination.mkdir(parents=True, exist_ok=True)
            (destination / 'python.exe').write_bytes(b'original python fixture')
            return python_lock

        with patch.object(integrated, 'prepare', side_effect=fake_python):
            result = integrated.prepare_resources(repo, distribution, python_archive=python_archive)
            destination = pathlib.Path(result['output'])
            self.assertTrue(destination.is_relative_to(repo))
            self.assertTrue(integrated.prepare_resources(repo, distribution, python_archive=python_archive)['prepared'])
            self.assertFalse((repo / '.artifacts/local-engine-review-20261101').exists())
            metadata = destination / 'component.json'
            modified = json.loads(metadata.read_text(encoding='utf-8'))
            modified['extra'] = 'original unknown field'
            metadata.write_text(json.dumps(modified), encoding='utf-8')
            with self.assertRaises(ValueError):
                integrated.prepare_resources(repo, distribution, python_archive=python_archive)
            self.assertEqual(json.loads(metadata.read_text(encoding='utf-8'))['extra'], 'original unknown field')


if __name__ == '__main__':
    unittest.main()
