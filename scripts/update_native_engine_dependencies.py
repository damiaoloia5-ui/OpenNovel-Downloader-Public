"""Maintainer-only security refresh of the pinned native component, preserving upstream.

Fetches registry dependencies; does not start the engine or access novel platforms.
The result must be audited, rebuilt and sandbox-tested before promoting its manifest.
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import tarfile
import tomllib

from build_offline_engine import source_archive
from rebuild_native_engine import build


def refresh(archive, output, repo):
    lock = json.loads((repo / 'scripts/research/native_engine_lock.json').read_text(encoding='utf-8'))
    expected = lock['files']['corresponding-source.tar.gz']
    if archive.stat().st_size != expected['bytes'] or hashlib.sha256(archive.read_bytes()).hexdigest() != expected['sha256']:
        raise ValueError('input is not the reviewed source archive')
    if output.exists():
        raise FileExistsError('choose a new directory; old component and evidence are preserved')
    source = output / 'source'
    source.mkdir(parents=True)
    with tarfile.open(archive, 'r:gz') as bundle:
        seen = set()
        for member in bundle:
            path = pathlib.PurePosixPath(member.name)
            if (not member.isfile() or path.is_absolute() or '..' in path.parts
                    or '\\' in member.name or ':' in member.name or len(path.parts) < 2
                    or path.parts[0] != 'opennovel-engine-source' or member.name in seen):
                raise ValueError('unexpected source member')
            seen.add(member.name)
            if path.parts[1] == 'vendor':
                continue
            target = source.joinpath(*path.parts[1:])
            target.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(member) as incoming, target.open('xb') as outgoing:
                shutil.copyfileobj(incoming, outgoing)
    inventory = json.loads((repo / 'third_party/local-engine-source-lock.json').read_text(encoding='utf-8'))
    for entry in inventory['files']:
        if hashlib.sha256((source / 'upstream' / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('upstream source differs from reviewed inventory')
    before = (source / 'Cargo.lock').read_bytes()
    # Update only the independent workspace lock; upstream files remain byte-identical.
    for package, version in [('rustls-webpki', '0.103.16'), ('webpki-roots', '1.0.10'),
                             ('webpki-root-certs', '1.0.10'), ('tokio', '1.53.3'), ('yoke-derive', '0.8.4')]:
        command = ['cargo', 'update', '--manifest-path', str(source / 'Cargo.toml'), '-p', package]
        if version: command += ['--precise', version]
        subprocess.run(command, cwd=source, check=True)
    with (output / 'vendor-config.txt').open('wb') as log:
        subprocess.run(['cargo', 'vendor', '--locked', '--versioned-dirs', 'vendor'], cwd=source, stdout=log, check=True)
    packages = tomllib.loads((source / 'Cargo.lock').read_text(encoding='utf-8'))['package']
    licenses = []
    for package in packages:
        if not package.get('source'): continue
        if package['source'] != 'registry+https://github.com/rust-lang/crates.io-index':
            raise ValueError('unreviewed dependency source')
        metadata = tomllib.loads((source / 'vendor' / (package['name'] + '-' + package['version']) / 'Cargo.toml').read_text(encoding='utf-8'))['package']
        licenses.append({'name': package['name'], 'version': package['version'], 'license': metadata.get('license'),
                         'license_file': metadata.get('license-file'), 'archive_sha256': package['checksum']})
    if any(not item['license'] and not item['license_file'] for item in licenses):
        raise ValueError('dependency license requires review')
    (source / 'dependency-licenses.json').write_text(json.dumps(licenses, indent=2), encoding='utf-8')
    provenance_path = source / 'provenance.json'
    provenance = json.loads(provenance_path.read_text(encoding='utf-8'))
    provenance.update(vendored_packages=len(licenses), previous_source_sha256=expected['sha256'],
                      original_workspace_lock_sha256=hashlib.sha256(before).hexdigest(),
                      refreshed_workspace_lock_sha256=hashlib.sha256((source / 'Cargo.lock').read_bytes()).hexdigest())
    provenance_path.write_text(json.dumps(provenance, indent=2), encoding='utf-8')
    with (source / 'REBUILD-NATIVE.md').open('a', encoding='utf-8') as instructions:
        instructions.write('\nSecurity refresh: root Cargo.lock is authoritative for this native build.\n'
                           'The unchanged upstream/rust/Cargo.lock is historical upstream material, not the native build lock.\n')
    binary = build(source, output / 'target')
    distribution = output / 'distribution'; distribution.mkdir()
    shutil.copy2(binary, distribution / 'engine.exe')
    source_archive(source, distribution / 'corresponding-source.tar.gz')
    lock['vendored_packages'] = len(licenses)
    lock['files'] = {path.name: {'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                     for path in distribution.iterdir()}
    (distribution / 'manifest.json').write_text(json.dumps(lock, indent=2) + '\n', encoding='utf-8')
    return distribution


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-archive', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    print(refresh(args.source_archive.resolve(), args.output.resolve(), pathlib.Path(__file__).resolve().parents[1]))
