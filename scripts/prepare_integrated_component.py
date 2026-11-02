"""Prepare application-adjacent resources without downloading or running the novel engine."""
import argparse
import hashlib
import json
import pathlib
import shutil
import re

from prepare_embedded_python import VERSION, prepare
from prepare_local_engine import RUNTIME_FILES, assemble
from reviewed_component_source import recover_source


def prepare_resources(repo, distribution, destination=None, python_archive=None):
    scripts = repo / 'scripts/research'
    python = repo / f'.artifacts/embedded-python-{VERSION}'
    archive = python_archive or repo / f'.cache/python-{VERSION}-embed-amd64.zip'
    if not archive.is_file():
        raise ValueError('Run scripts/prepare_embedded_python.py --download as a maintainer first')
    python_lock = prepare(archive.read_bytes(), python)
    expected_lock = json.loads((repo / 'third_party/python-embed-lock.json').read_text(encoding='utf-8'))
    if python_lock != expected_lock:
        raise ValueError('reviewed Python runtime lock mismatch')
    native = json.loads((distribution / 'manifest.json').read_text(encoding='utf-8')).get('backend') == 'windows-appcontainer'
    folder = 'native-engine' if native else 'offline-engine'
    lock_path = scripts / ('native_engine_lock.json' if native else 'engine_bundle_lock.json')
    engine_lock = json.loads(lock_path.read_text(encoding='utf-8'))
    if json.loads((distribution / 'manifest.json').read_text(encoding='utf-8')) != engine_lock:
        raise ValueError('offline build differs from reviewed runtime lock')
    for name, expected in engine_lock['files'].items():
        path = distribution / name
        if path.stat().st_size != expected['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest() != expected['sha256']:
            raise ValueError('offline distribution changed')
    version = json.loads((repo / 'package.json').read_text(encoding='utf-8'))['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?', version):
        raise ValueError('invalid package version')
    destination = destination or repo / '.artifacts' / ('integrated-resources-' + version) / 'opennovel-engine'
    inventory_path = repo / 'third_party/local-engine-source-lock.json'
    inventory = json.loads(inventory_path.read_text(encoding='utf-8'))
    if inventory['commit'] != engine_lock['candidate_commit']:
        raise ValueError('source revision differs from reviewed engine')
    source = recover_source(distribution / 'corresponding-source.tar.gz',
                            engine_lock['files']['corresponding-source.tar.gz'], inventory,
                            repo / '.cache' / ('component-source-' + inventory['commit']))
    expected_manifest = {'schema_version': 1, 'candidate_commit': inventory['commit'],
                         'runtime_sha256': {name: hashlib.sha256((scripts / name).read_bytes()).hexdigest() for name in RUNTIME_FILES},
                         'inventory_sha256': hashlib.sha256(inventory_path.read_bytes()).hexdigest(),
                         'python': 'python/python.exe', 'python_bundled': True, 'python_version': VERSION,
                         'engine_binary_bundled': True, 'runtime_ready_checked': False,
                         'engine_binary_license': 'GPL-3.0',
                         'corresponding_source': folder + '/corresponding-source.tar.gz'}
    if not destination.exists():
        assemble(destination, source, inventory_path, scripts, repo / 'LICENSE')
        shutil.copytree(python, destination / 'python')
        offline = destination / folder
        offline.mkdir()
        for name in engine_lock['files']:
            shutil.copy2(distribution / name, offline / name)
        # Use the build's exact lock bytes, including repository line endings.
        (offline / 'manifest.json').write_bytes(lock_path.read_bytes())
        manifest_path = destination / 'component.json'
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
        manifest.update(python='python/python.exe', python_bundled=True, python_version=VERSION,
                        engine_binary_bundled=True, engine_binary_license='GPL-3.0',
                        corresponding_source=folder + '/corresponding-source.tar.gz')
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    expected = {'inventory.json', 'OPENNOVEL-LICENSE.txt', 'component.json'}
    if (json.loads((destination / 'component.json').read_text(encoding='utf-8')) != expected_manifest
            or (destination / 'inventory.json').read_bytes() != inventory_path.read_bytes()
            or (destination / 'OPENNOVEL-LICENSE.txt').read_bytes() != (repo / 'LICENSE').read_bytes()):
        raise ValueError('prepared component metadata or notices changed; refusing overwrite')
    expected.add(folder + '/manifest.json')
    if (destination / folder / 'manifest.json').read_bytes() != lock_path.read_bytes():
        raise ValueError('offline manifest changed')
    for name, entry in engine_lock['files'].items():
        expected.add(folder + '/' + name)
        path = destination / folder / name
        if path.stat().st_size != entry['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('prepared offline component changed')
    for name in RUNTIME_FILES:
        expected.add('runtime/' + name)
        if (destination / 'runtime' / name).read_bytes() != (scripts / name).read_bytes():
            raise ValueError('existing generated component differs; retain it and prepare a new resource directory')
    for name, digest in python_lock['files'].items():
        expected.add('python/' + name)
        if hashlib.sha256((destination / 'python' / name).read_bytes()).hexdigest() != digest:
            raise ValueError('bundled Python changed')
    for entry in inventory['files']:
        expected.add('source/' + entry['path'])
        if hashlib.sha256((destination / 'source' / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('bundled source changed')
    actual = {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file()}
    if actual != expected or any(p.is_symlink() for p in destination.rglob('*')):
        raise ValueError('unexpected generated resource; refusing to bundle')
    return {'prepared': True, 'python_bundled': True, 'files': len(actual),
            'engine_binary_bundled': True, 'development_volumes_required': False, 'docker_required': not native,
            'output': str(destination)}


def main():
    repo = pathlib.Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--distribution', type=pathlib.Path, required=True,
                        help='local folder containing the pinned native or legacy container distribution')
    parser.add_argument('--python-archive', type=pathlib.Path, help='pinned official embedded Python ZIP')
    parser.add_argument('--output', type=pathlib.Path, help='absolute component output; default uses package version')
    args = parser.parse_args()
    if args.output is not None and not args.output.is_absolute():
        parser.error('--output must be absolute')
    print(json.dumps(prepare_resources(repo, args.distribution.resolve(), args.output,
                                      args.python_archive.resolve() if args.python_archive else None)))


if __name__ == '__main__':
    main()
