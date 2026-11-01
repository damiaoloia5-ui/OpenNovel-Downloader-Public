"""Maintainer-only: assemble pinned corresponding source and vendored registry dependencies."""
import argparse
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
import urllib.request

REPO = pathlib.Path(__file__).resolve().parents[1]
OUTPUT = REPO / '.artifacts/offline-engine-20261101'
BASE = 'sha256:f77ac9e44ae96ef2c90b8053ea08c31f8be030f824196b0ae4db6d462c84e51f'
BUILDER = 'sha256:e18a79fc84dfcfc3ab5ba72290398a644c135c97eaa881447fddc354ee4701a3'


def unpack_crate(archive, destination, name, checksum):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
        raise ValueError('locked crate archive changed')
    if destination.exists():
        raise FileExistsError('refusing to overwrite vendored source')
    with tarfile.open(archive, 'r:gz') as source:
        entries = source.getmembers()
        if len(entries) > 20000 or sum(m.size for m in entries) > 100 * 1024 * 1024:
            raise ValueError('crate expansion too large')
        files = {}
        for member in entries:
            path = pathlib.PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != name or '\\' in member.name or ':' in member.name:
                raise ValueError('crate archive path refused')
            if member.isdir():
                continue
            if not member.isfile() or len(path.parts) < 2:
                raise ValueError('crate archive link or special file refused')
            relative = '/'.join(path.parts[1:])
            if relative in files:
                raise ValueError('duplicate crate archive path')
            files[relative] = (member, None)
        destination.mkdir(parents=True)
        hashes = {}
        for relative, (member, _) in files.items():
            data = source.extractfile(member).read()
            target = destination.joinpath(*pathlib.PurePosixPath(relative).parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            hashes[relative] = hashlib.sha256(data).hexdigest()
    (destination / '.cargo-checksum.json').write_text(json.dumps({'files': hashes, 'package': checksum}), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--download-missing', action='store_true')
    args = parser.parse_args()
    sys.path.insert(0, str(REPO / 'scripts/research'))
    import engine_runtime_config as config
    manifest = config.verify_source()
    lock = tomllib.loads((config.SOURCE / 'rust/Cargo.lock').read_text(encoding='utf-8'))
    crates = [p for p in lock['package'] if p.get('source', '').startswith('registry+')]
    if any(p.get('source') and p['source'] != 'registry+https://github.com/rust-lang/crates.io-index' for p in lock['package']):
        raise ValueError('non-reviewed dependency source')
    expected = {p['name'] + '-' + p['version'] + '.crate': p['checksum'] for p in crates}
    if any(not re.fullmatch(r'[A-Za-z0-9_.+\-]+', name) for name in expected):
        raise ValueError('crate filename refused')
    archives = OUTPUT / 'crate-archives'
    archives.mkdir(parents=True, exist_ok=True)
    # Export only the requested public crate archives, never Cargo credentials or config.
    exporter = '''import hashlib,json,pathlib,sys
expected=json.load(sys.stdin); missing=[]
for name,digest in expected.items():
 matches=list(pathlib.Path('/cargo/registry/cache').glob('*/'+name))
 valid=next((p for p in matches if hashlib.sha256(p.read_bytes()).hexdigest()==digest),None)
 if valid is None: missing.append(name)
 else: pathlib.Path('/export',name).write_bytes(valid.read_bytes())
print(json.dumps({'missing':missing}))
'''
    result = subprocess.run([config.DOCKER, 'run', '--rm', '-i', '--pull=never', '--network=none', '--read-only',
        '--user=10001:10001', '--cap-drop=ALL', '--security-opt=no-new-privileges', '--log-driver=none',
        '--mount', f'type=volume,source={config.CACHE},target=/cargo,readonly',
        '--mount', f'type=bind,source={archives},target=/export', BASE, 'python', '-c', exporter],
        input=json.dumps(expected), text=True, capture_output=True, timeout=60, check=True)
    missing = json.loads(result.stdout)['missing']
    print(json.dumps({'locked_crates': len(crates), 'missing_archives': len(missing)}), flush=True)
    if missing and not args.download_missing:
        raise ValueError('missing locked archives; maintainer may use --download-missing')
    for package in crates:
        name = package['name'] + '-' + package['version']
        archive = archives / (name + '.crate')
        if not archive.is_file():
            url = f'https://static.crates.io/crates/{package["name"]}/{name}.crate'
            with urllib.request.urlopen(url, timeout=40) as response:
                data = response.read(16 * 1024 * 1024 + 1)
            if len(data) > 16 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != package['checksum']:
                raise ValueError('downloaded crate checksum mismatch')
            with archive.open('xb') as output:
                output.write(data)
    source = OUTPUT / 'corresponding-source'
    if source.exists():
        raise FileExistsError('source package already exists; preserve it before rebuilding')
    shutil.copytree(config.SOURCE, source / 'upstream')
    examples = source / 'upstream/rust/examples'
    examples.mkdir(exist_ok=True)
    wrapper = REPO / 'scripts/research/engine_probe_examples/opennovel_engine_service.rs'
    shutil.copy2(wrapper, examples / wrapper.name)
    shutil.copy2(REPO / 'LICENSE', source / 'OPENNOVEL-LICENSE.txt')
    licenses = []
    for package in crates:
        name = package['name'] + '-' + package['version']
        target = source / 'vendor' / name
        unpack_crate(archives / (name + '.crate'), target, name, package['checksum'])
        metadata = tomllib.loads((target / 'Cargo.toml').read_text(encoding='utf-8'))['package']
        licenses.append({'name': package['name'], 'version': package['version'], 'license': metadata.get('license'),
                         'license_file': metadata.get('license-file'), 'archive_sha256': package['checksum']})
    (source / 'cargo-config.toml').write_text('[source.crates-io]\nreplace-with = "vendored-sources"\n[source.vendored-sources]\ndirectory = "/bundle/vendor"\n', encoding='utf-8')
    (source / 'dependency-licenses.json').write_text(json.dumps(licenses, indent=2), encoding='utf-8')
    provenance = {'candidate_commit': manifest['commit'], 'candidate_files': len(manifest['files']),
                  'wrapper_sha256': hashlib.sha256(wrapper.read_bytes()).hexdigest(), 'builder_image': BUILDER,
                  'runtime_base_image': BASE, 'vendored_packages': len(crates), 'engine_binary_license': 'GPL-3.0',
                  'unknown_dependency_licenses': [p['name'] for p in licenses if not p['license'] and not p['license_file']]}
    (source / 'provenance.json').write_text(json.dumps(provenance, indent=2), encoding='utf-8')
    print(json.dumps(provenance), flush=True)


if __name__ == '__main__':
    main()
