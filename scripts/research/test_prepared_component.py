"""Original preparation fixtures; no Docker, real candidate or executable runs."""
import hashlib
import importlib.util
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('prepare_local_engine', pathlib.Path(__file__).resolve().parents[1] / 'prepare_local_engine.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.source = self.root / 'source'
        self.source.mkdir()
        (self.source / 'LICENSE').write_text('Original license fixture', encoding='utf-8')
        self.scripts = self.root / 'scripts'
        self.scripts.mkdir()
        for name in prepare.RUNTIME_FILES:
            (self.scripts / name).write_text('# Original runtime fixture: ' + name, encoding='utf-8')
        self.inventory = self.root / 'inventory.json'
        self.inventory.write_text(json.dumps({'commit': 'fixture', 'files': []}), encoding='utf-8')
        self.license = self.root / 'LICENSE'
        self.license.write_text('Original host license fixture', encoding='utf-8')
        self.destination = self.root / 'component'

    def assemble(self, executable=None):
        return prepare.assemble(self.destination, self.source, self.inventory, self.scripts, self.license, executable)

    def test_copies_only_runtime_allowlist_and_preserves_license_and_inventory(self):
        (self.scripts / 'unrelated.py').write_text('not copied', encoding='utf-8')
        manifest = self.assemble()
        self.assertEqual({p.name for p in (self.destination / 'runtime').iterdir()}, set(prepare.RUNTIME_FILES))
        self.assertEqual((self.destination / 'source/LICENSE').read_bytes(), (self.source / 'LICENSE').read_bytes())
        self.assertEqual((self.destination / 'inventory.json').read_bytes(), self.inventory.read_bytes())
        self.assertFalse(manifest['engine_binary_bundled'])
        self.assertFalse(manifest['runtime_ready_checked'])
        for name, digest in manifest['runtime_sha256'].items():
            self.assertEqual(digest, hashlib.sha256((self.destination / 'runtime' / name).read_bytes()).hexdigest())

    def test_existing_output_is_preserved(self):
        self.destination.mkdir()
        sentinel = self.destination / 'keep.txt'
        sentinel.write_text('keep original', encoding='utf-8')
        with self.assertRaises(FileExistsError):
            self.assemble()
        self.assertEqual(sentinel.read_text(encoding='utf-8'), 'keep original')

    def test_missing_input_creates_no_partial_directory(self):
        (self.scripts / prepare.RUNTIME_FILES[0]).unlink()
        with self.assertRaises(ValueError):
            self.assemble()
        self.assertFalse(self.destination.exists())

    def test_optional_application_gets_an_isolated_library_launcher(self):
        executable = self.root / 'app.exe'
        executable.write_bytes(b'Original fixture, never executed')
        manifest = self.assemble(executable)
        self.assertEqual((self.destination / 'OpenNovel-Preview.exe').read_bytes(), executable.read_bytes())
        launch = (self.destination / 'Start-Isolated-Preview.cmd').read_text(encoding='utf-8')
        self.assertIn('OPENNOVEL_ENGINE_ROOT=%~dp0', launch)
        self.assertIn('preview-library\\library.db', launch)
        self.assertEqual(manifest['application_sha256'], hashlib.sha256(executable.read_bytes()).hexdigest())


if __name__ == '__main__':
    unittest.main()
