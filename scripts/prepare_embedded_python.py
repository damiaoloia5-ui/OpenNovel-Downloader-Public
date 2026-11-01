"""Maintainer-only: prepare the pinned PSF runtime; application startup never downloads it."""
import argparse
import hashlib
import io
import json
import pathlib
import urllib.request
import zipfile

VERSION = '3.14.10'
URL = f'https://www.python.org/ftp/python/{VERSION}/python-{VERSION}-embed-amd64.zip'
SHA256 = '90905e64a840557d8c1bc4f5b74db3d7ee85e3ffb217611cac1c7813acea4159'
PTH = b'python314.zip\n.\n../runtime\n'


def verified_files(data):
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise ValueError('official archive SHA256 mismatch')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if not entries or len(entries) > 100 or sum(x.file_size for x in entries) > 100 * 1024 * 1024:
            raise ValueError('runtime archive size refused')
        if any('/' in x.filename or '\\' in x.filename or ':' in x.filename or x.filename in ('.', '..') for x in entries):
            raise ValueError('runtime archive must contain only flat files')
        if len({x.filename for x in entries}) != len(entries):
            raise ValueError('duplicate runtime archive entry')
        files = {x.filename: archive.read(x) for x in entries}
    if not {'python.exe', 'python314.dll', 'python314.zip', 'python314._pth', 'LICENSE.txt'} <= files.keys():
        raise ValueError('official runtime files missing')
    # Keep isolated import paths; do not import site, user packages or environment paths.
    files['python314._pth'] = PTH
    return files


def prepare(data, destination):
    files = verified_files(data)
    manifest = {'schema_version': 1, 'version': VERSION, 'url': URL, 'archive_sha256': SHA256,
                'files': {name: hashlib.sha256(raw).hexdigest() for name, raw in sorted(files.items())}}
    if destination.exists():
        if {p.name for p in destination.iterdir()} != set(files) or any(
                not (destination / name).is_file() or (destination / name).read_bytes() != raw for name, raw in files.items()):
            raise ValueError('existing runtime differs; refusing overwrite')
    else:
        destination.mkdir(parents=True)
        for name, raw in files.items():
            (destination / name).write_bytes(raw)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--download', action='store_true', help='explicitly download the pinned official archive')
    args = parser.parse_args()
    repo = pathlib.Path(__file__).resolve().parents[1]
    archive = repo / '.cache' / f'python-{VERSION}-embed-amd64.zip'
    if not archive.is_file():
        if not args.download:
            parser.error('pinned archive missing; use --download as a maintainer')
        archive.parent.mkdir(exist_ok=True)
        with urllib.request.urlopen(URL, timeout=40) as response:
            if response.url != URL:
                raise ValueError('runtime download redirected')
            data = response.read(20 * 1024 * 1024 + 1)
        verified_files(data)
        with archive.open('xb') as output:
            output.write(data)
    manifest = prepare(archive.read_bytes(), repo / f'.artifacts/embedded-python-{VERSION}')
    lock = repo / 'third_party/python-embed-lock.json'
    encoded = json.dumps(manifest, indent=2) + '\n'
    if lock.exists() and lock.read_text(encoding='utf-8') != encoded:
        raise ValueError('runtime lock differs; review the version update separately')
    if not lock.exists():
        lock.write_text(encoded, encoding='utf-8')
    print(json.dumps({'prepared': True, 'version': VERSION, 'files': len(manifest['files']), 'sha256': SHA256}))


if __name__ == '__main__':
    main()
