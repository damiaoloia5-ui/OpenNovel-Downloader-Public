"""Original crate fixtures; no Docker, registry access or candidate execution."""
import hashlib
import importlib.util
import io
import json
import pathlib
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('prepare_engine_sources', pathlib.Path(__file__).resolve().parents[1] / 'prepare_engine_sources.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class EngineSourceTests(unittest.TestCase):
    def archive(self, root, name='fixture-1.0/Cargo.toml', kind=tarfile.REGTYPE):
        archive = root / 'fixture.crate'
        with tarfile.open(archive, 'w:gz') as stream:
            data = b'[package]\nname="fixture"\nversion="1.0"\nlicense="MIT"\n'
            entry = tarfile.TarInfo(name)
            entry.type = kind
            entry.size = len(data) if kind == tarfile.REGTYPE else 0
            entry.linkname = '/outside' if kind == tarfile.SYMTYPE else ''
            stream.addfile(entry, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
        return archive, hashlib.sha256(archive.read_bytes()).hexdigest()

    def test_valid_crate_gets_locked_vendor_checksums_without_overwriting(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            archive, digest = self.archive(root)
            destination = root / 'vendor'
            prepare.unpack_crate(archive, destination, 'fixture-1.0', digest)
            manifest = json.loads((destination / '.cargo-checksum.json').read_text())
            self.assertEqual(manifest['package'], digest)
            self.assertEqual(manifest['files']['Cargo.toml'], hashlib.sha256((destination / 'Cargo.toml').read_bytes()).hexdigest())
            with self.assertRaises(FileExistsError):
                prepare.unpack_crate(archive, destination, 'fixture-1.0', digest)

    def test_bad_digest_paths_and_links_create_no_vendor_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            archive, _ = self.archive(root)
            with self.assertRaises(ValueError):
                prepare.unpack_crate(archive, root / 'bad-hash', 'fixture-1.0', '0' * 64)
            self.assertFalse((root / 'bad-hash').exists())
            for index, (name, kind) in enumerate([('../outside', tarfile.REGTYPE), ('/absolute', tarfile.REGTYPE),
                                                ('fixture-1.0/../outside', tarfile.REGTYPE), ('fixture-1.0/link', tarfile.SYMTYPE)]):
                archive, digest = self.archive(root, name, kind)
                destination = root / str(index)
                with self.assertRaises(ValueError):
                    prepare.unpack_crate(archive, destination, 'fixture-1.0', digest)
                self.assertFalse(destination.exists())


if __name__ == '__main__':
    unittest.main()
