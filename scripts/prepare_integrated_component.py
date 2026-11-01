"""Prepare application-adjacent resources without downloading or running the novel engine."""
import hashlib
import json
import pathlib
import shutil
import sys

from prepare_embedded_python import VERSION, prepare
from prepare_local_engine import RUNTIME_FILES, assemble


def main():
    repo = pathlib.Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo / 'scripts/research'))
    import engine_runtime_config as config
    config.verify_source()
    python = repo / f'.artifacts/embedded-python-{VERSION}'
    archive = repo / f'.cache/python-{VERSION}-embed-amd64.zip'
    if not archive.is_file():
        raise ValueError('Run scripts/prepare_embedded_python.py --download as a maintainer first')
    python_lock = prepare(archive.read_bytes(), python)
    expected_lock = json.loads((repo / 'third_party/python-embed-lock.json').read_text(encoding='utf-8'))
    if python_lock != expected_lock:
        raise ValueError('reviewed Python runtime lock mismatch')
    lock_path = config.SCRIPTS / 'engine_bundle_lock.json'
    engine_lock = json.loads(lock_path.read_text(encoding='utf-8'))
    distribution = repo / '.artifacts/offline-engine-20261101/distribution-v2'
    if json.loads((distribution / 'manifest.json').read_text(encoding='utf-8')) != engine_lock:
        raise ValueError('offline build differs from reviewed runtime lock')
    for name, expected in engine_lock['files'].items():
        path = distribution / name
        if path.stat().st_size != expected['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest() != expected['sha256']:
            raise ValueError('offline distribution changed')
    destination = repo / '.artifacts/offline-resources/opennovel-engine'
    if not destination.exists():
        assemble(destination, config.SOURCE, config.PREVIOUS / 'inventory.json', config.SCRIPTS, repo / 'LICENSE')
        shutil.copytree(python, destination / 'python')
        offline = destination / 'offline-engine'
        offline.mkdir()
        for name in engine_lock['files']:
            shutil.copy2(distribution / name, offline / name)
        # Use the build's exact lock bytes, including repository line endings.
        (offline / 'manifest.json').write_bytes(lock_path.read_bytes())
        manifest_path = destination / 'component.json'
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
        manifest.update(python='python/python.exe', python_bundled=True, python_version=VERSION,
                        engine_binary_bundled=True, engine_binary_license='GPL-3.0',
                        corresponding_source='offline-engine/corresponding-source.tar.gz')
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    expected = {'inventory.json', 'OPENNOVEL-LICENSE.txt', 'component.json'}
    expected.add('offline-engine/manifest.json')
    if (destination / 'offline-engine/manifest.json').read_bytes() != lock_path.read_bytes():
        raise ValueError('offline manifest changed')
    for name, entry in engine_lock['files'].items():
        expected.add('offline-engine/' + name)
        path = destination / 'offline-engine' / name
        if path.stat().st_size != entry['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('prepared offline component changed')
    inventory = json.loads((config.PREVIOUS / 'inventory.json').read_text(encoding='utf-8'))
    for name in RUNTIME_FILES:
        expected.add('runtime/' + name)
        if (destination / 'runtime' / name).read_bytes() != (config.SCRIPTS / name).read_bytes():
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
    print(json.dumps({'prepared': True, 'python_bundled': True, 'files': len(actual),
                      'engine_binary_bundled': True, 'development_volumes_required': False, 'docker_required': True}))


if __name__ == '__main__':
    main()
