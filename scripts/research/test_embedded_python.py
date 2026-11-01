"""Original runtime archive fixtures; no download, interpreter or candidate execution."""
import hashlib
import importlib.util
import io
import pathlib
import tempfile
import unittest
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('prepare_embedded_python', pathlib.Path(__file__).resolve().parents[1] / 'prepare_embedded_python.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class EmbeddedPythonTests(unittest.TestCase):
    def archive(self, extra=None):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            for name in ['python.exe', 'python314.dll', 'python314.zip', 'python314._pth', 'LICENSE.txt']:
                archive.writestr(name, b'Original fixture, never executed')
            if extra:
                archive.writestr(extra, b'original')
        return stream.getvalue()

    def test_unknown_archive_is_refused_before_extracting(self):
        with self.assertRaisesRegex(ValueError, 'SHA256'):
            prepare.verified_files(self.archive())

    def test_flat_runtime_keeps_isolated_imports_and_reuses_identical_output(self):
        data = self.archive()
        with tempfile.TemporaryDirectory() as root, patch.object(prepare, 'SHA256', hashlib.sha256(data).hexdigest()):
            destination = pathlib.Path(root) / 'python'
            manifest = prepare.prepare(data, destination)
            self.assertEqual((destination / 'python314._pth').read_bytes(), prepare.PTH)
            self.assertNotIn(b'import site', prepare.PTH)
            self.assertEqual(manifest, prepare.prepare(data, destination))
            (destination / 'python.exe').write_bytes(b'original obstacle')
            with self.assertRaisesRegex(ValueError, 'refusing overwrite'):
                prepare.prepare(data, destination)
            self.assertEqual((destination / 'python.exe').read_bytes(), b'original obstacle')

    def test_archive_paths_cannot_escape_runtime_directory(self):
        for path in ['../outside', 'nested/entry', r'C:\outside']:
            data = self.archive(path)
            with patch.object(prepare, 'SHA256', hashlib.sha256(data).hexdigest()):
                with self.assertRaisesRegex(ValueError, 'flat files'):
                    prepare.verified_files(data)


if __name__ == '__main__':
    unittest.main()
