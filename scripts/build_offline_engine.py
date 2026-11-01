"""Explicit maintainer build: offline GPL component, source archive and pinned image export."""
import argparse
import gzip
import hashlib
import io
import json
import pathlib
import shutil
import subprocess
import sys
import tarfile

from prepare_engine_sources import BASE, BUILDER, OUTPUT, REPO

TARGET = 'opennovel-offline-engine-20261101-target'
OWNER = 'opennovel-offline-engine-20261101'


def docker(executable, *args, data=None, timeout=60):
    result = subprocess.run([executable, *args], input=data, capture_output=True, timeout=timeout)
    if result.returncode:
        (OUTPUT / 'build-error.log').write_bytes(result.stderr)
        raise RuntimeError(f'Docker {args[0]} failed; inspect the local build log')
    return (result.stdout + (result.stderr if args[0] == 'logs' else b'')).decode('utf-8').strip()


def source_archive(source, output):
    with output.open('xb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed:
        with tarfile.open(fileobj=compressed, mode='w|') as archive:
            for path in sorted(source.rglob('*')):
                if path.is_symlink():
                    raise ValueError('source symlink refused')
                if not path.is_file():
                    continue
                info = tarfile.TarInfo('opennovel-engine-source/' + path.relative_to(source).as_posix())
                info.size = path.stat().st_size
                info.mode = 0o644
                with path.open('rb') as contents:
                    archive.addfile(info, contents)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', type=int, default=1)
    args = parser.parse_args()
    if not 1 <= args.revision <= 9999:
        raise ValueError('invalid build revision')
    distribution = OUTPUT / f'distribution-v{args.revision}'
    if distribution.exists():
        raise FileExistsError('distribution already exists; use a new revision and preserve previous output')
    sys.path.insert(0, str(REPO / 'scripts/research'))
    import engine_runtime_config as config
    original = config.verify_source()
    source = OUTPUT / 'corresponding-source'
    provenance = json.loads((source / 'provenance.json').read_text(encoding='utf-8'))
    if provenance['candidate_commit'] != original['commit'] or provenance['unknown_dependency_licenses']:
        raise ValueError('source/license review not ready')
    for entry in original['files']:
        if hashlib.sha256((source / 'upstream' / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('candidate source changed')
    wrapper = source / 'upstream/rust/examples/opennovel_engine_service.rs'
    if hashlib.sha256(wrapper.read_bytes()).hexdigest() != provenance['wrapper_sha256']:
        raise ValueError('wrapper source changed')
    inspected = subprocess.run([config.DOCKER, 'volume', 'inspect', TARGET], capture_output=True)
    if inspected.returncode == 0:
        if json.loads(inspected.stdout)[0].get('Labels', {}).get('opennovel.owner') != OWNER:
            raise ValueError('existing build volume has another owner')
    else:
        docker(config.DOCKER, 'volume', 'create', '--label', 'opennovel.owner=' + OWNER, TARGET)
        # Root is used only to set ownership of this new, empty build volume.
        docker(config.DOCKER, 'run', '--rm', '--pull=never', '--network=none', '--read-only',
               '--user=0:0', '--cap-drop=ALL', '--cap-add=CHOWN', '--security-opt=no-new-privileges',
               '--log-driver=none', '--mount', f'type=volume,source={TARGET},target=/target',
               BASE, 'chown', '10001:10001', '/target')
    build = docker(config.DOCKER, 'create', '--pull=never', '--network=none', '--read-only',
        '--user=10001:10001', '--cap-drop=ALL', '--security-opt=no-new-privileges',
        '--memory=2g', '--cpus=2', '--pids-limit=256', '--tmpfs', '/tmp:rw,nosuid,nodev,size=268435456',
        '--env', 'CARGO_HOME=/tmp/cargo', '--env', 'CARGO_TARGET_DIR=/target', '--env', 'CARGO_INCREMENTAL=0',
        '--env', 'CARGO_BUILD_JOBS=2', '--mount', f'type=bind,source={source},target=/bundle,readonly',
        '--mount', f'type=volume,source={TARGET},target=/target', BUILDER,
        'cargo', 'build', '--locked', '--offline', '--release', '--target', 'x86_64-unknown-linux-gnu',
        '--manifest-path', '/bundle/upstream/rust/Cargo.toml', '--config', '/bundle/cargo-config.toml',
        '--example', 'opennovel_engine_service')
    context = OUTPUT / 'image-input'
    context.mkdir(exist_ok=True)
    try:
        state = json.loads(docker(config.DOCKER, 'inspect', build))[0]
        host = state['HostConfig']
        assert host['NetworkMode'] == 'none' and host['ReadonlyRootfs'] and host['CapDrop'] == ['ALL']
        assert not host['Privileged'] and not host['PortBindings'] and state['Config']['User'] == '10001:10001'
        docker(config.DOCKER, 'start', build)
        print(json.dumps({'building_offline': True, 'container': build}), flush=True)
        code = int(docker(config.DOCKER, 'wait', build, timeout=900))
        (OUTPUT / 'release-build.log').write_text(docker(config.DOCKER, 'logs', build), encoding='utf-8')
        if code:
            raise RuntimeError('offline release build failed; see release-build.log')
        docker(config.DOCKER, 'cp', build + ':/target/x86_64-unknown-linux-gnu/release/examples/opennovel_engine_service',
               str(context / 'opennovel_engine_service'))
        (OUTPUT / 'build-isolation.json').write_text(json.dumps({'image': state['Image'], 'network': host['NetworkMode'],
            'read_only_root': host['ReadonlyRootfs'], 'user': state['Config']['User'], 'cap_drop': host['CapDrop'],
            'mounts': [{'path': m['Destination'], 'rw': m['RW']} for m in state['Mounts']], 'exit': code}, indent=2), encoding='utf-8')
    finally:
        state = json.loads(docker(config.DOCKER, 'inspect', build))[0]['State']
        if state['Running']:
            docker(config.DOCKER, 'stop', '--time', '3', build)
        docker(config.DOCKER, 'rm', build)
    # A stopped base-image container receives only a verified build result and notices.
    # Neither its default command nor the candidate is run during image assembly.
    assembly = docker(config.DOCKER, 'create', '--network=none', '--user=10001:10001', '--cap-drop=ALL', BASE)
    try:
        payload = io.BytesIO()
        with tarfile.open(fileobj=payload, mode='w') as archive:
            items = [('opennovel_engine_service', (context / 'opennovel_engine_service').read_bytes(), 0o555),
                     ('COPYING', (source / 'upstream/LICENSE').read_bytes(), 0o444),
                     ('OPENNOVEL-LICENSE.txt', (REPO / 'LICENSE').read_bytes(), 0o444),
                     ('dependency-licenses.json', (source / 'dependency-licenses.json').read_bytes(), 0o444)]
            for name, contents, mode in items:
                info = tarfile.TarInfo('opennovel/' + name)
                info.size, info.mode = len(contents), mode
                archive.addfile(info, io.BytesIO(contents))
        docker(config.DOCKER, 'cp', '-', assembly + ':/opt', data=payload.getvalue())
        image = docker(config.DOCKER, 'commit', '--change', 'USER 10001:10001', '--change', 'WORKDIR /tmp',
                       '--change', 'LABEL org.opennovel.component=readonly-engine', assembly, timeout=120)
    finally:
        docker(config.DOCKER, 'rm', assembly)
    image_info = json.loads(docker(config.DOCKER, 'image', 'inspect', image))[0]
    assert image_info['Os'] == 'linux' and image_info['Architecture'] == 'amd64'
    (source / 'REBUILD.md').write_text(f'''# Independent GPL component source

The engine binary as a combined work is supplied under GPL version 3; upstream LICENSE is in upstream/.
The original OpenNovel wrapper and build tooling retain Apache-2.0, included separately.
All {provenance['vendored_packages']} locked registry dependencies and their license files are under vendor/.

Builder: official rust:1.96-slim-bookworm, pinned {BUILDER}.
Use a new output volume owned by UID 10001. Mount this source directory read-only at /bundle,
mount the output at /target, set CARGO_HOME=/tmp/cargo and CARGO_TARGET_DIR=/target,
and build as UID 10001 with --network=none, read-only root and all capabilities dropped:

    cargo build --locked --offline --release --target x86_64-unknown-linux-gnu --manifest-path /bundle/upstream/rust/Cargo.toml --config /bundle/cargo-config.toml --example opennovel_engine_service

The resulting binary is /target/x86_64-unknown-linux-gnu/release/examples/opennovel_engine_service.
Image assembly uses base {BASE} and adds only the binary and license notices at /opt/opennovel.
The application runs the component in a separate restricted container, not linked into the host.
No account, device registration or entitlement action is exposed by the wrapper or host gate.
''', encoding='utf-8')
    shutil.copy2(pathlib.Path(__file__), source / 'BUILD-TOOL.py')
    shutil.copy2(REPO / 'scripts/rebuild_engine_source.py', source / 'REBUILD.py')
    with (source / 'REBUILD.md').open('a', encoding='utf-8') as instructions:
        instructions.write('\nWith Python 3.11+ and the two pinned toolchain/base images installed, run `python REBUILD.py` from this extracted directory. It creates new output and a dedicated volume, performs an offline build, and preserves the source.\n')
    distribution.mkdir()
    source_archive(source, distribution / 'corresponding-source.tar.gz')
    raw_image = OUTPUT / 'runtime-image.tar'
    docker(config.DOCKER, 'image', 'save', '--output', str(raw_image), image, timeout=120)
    with raw_image.open('rb') as input_file, (distribution / 'runtime-image.tar.gz').open('xb') as output_file:
        with gzip.GzipFile(fileobj=output_file, mode='wb', mtime=0, filename='') as compressed:
            shutil.copyfileobj(input_file, compressed)
    files = {p.name: {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in distribution.iterdir()}
    manifest = {'schema_version': 1, 'image_id': image_info['Id'], 'base_image': BASE, 'builder_image': BUILDER,
        'binary_path': '/opt/opennovel/opennovel_engine_service', 'binary_sha256': hashlib.sha256((context / 'opennovel_engine_service').read_bytes()).hexdigest(),
        'candidate_commit': provenance['candidate_commit'], 'binary_license': 'GPL-3.0', 'vendored_packages': provenance['vendored_packages'], 'files': files}
    (distribution / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(manifest), flush=True)


if __name__ == '__main__':
    main()
