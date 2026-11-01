"""Run beside REBUILD.md in an extracted source package; build with an installed pinned toolchain image."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import uuid


def main():
    root = pathlib.Path(__file__).resolve().parent
    provenance = json.loads((root / 'provenance.json').read_text(encoding='utf-8'))
    docker = shutil.which('docker')
    if not docker:
        raise RuntimeError('Docker CLI required; this source rebuild tool does not install it')
    output = root / ('rebuilt-' + uuid.uuid4().hex)
    output.mkdir()
    volume = 'opennovel-source-rebuild-' + uuid.uuid4().hex
    def run(*args):
        return subprocess.check_output([docker, *args], text=True).strip()
    run('image', 'inspect', provenance['builder_image'])
    run('volume', 'create', volume)
    run('run', '--rm', '--pull=never', '--network=none', '--read-only', '--user=0:0', '--cap-drop=ALL',
        '--cap-add=CHOWN', '--security-opt=no-new-privileges', '--mount', f'type=volume,source={volume},target=/target',
        provenance['runtime_base_image'], 'chown', '10001:10001', '/target')
    container = run('create', '--network=none', '--read-only', '--user=10001:10001', '--cap-drop=ALL',
        '--security-opt=no-new-privileges', '--memory=2g', '--cpus=2', '--pids-limit=256',
        '--tmpfs', '/tmp:rw,nosuid,nodev,size=268435456', '--env', 'CARGO_HOME=/tmp/cargo',
        '--env', 'CARGO_TARGET_DIR=/target', '--env', 'CARGO_BUILD_JOBS=2',
        '--mount', f'type=bind,source={root},target=/bundle,readonly',
        '--mount', f'type=volume,source={volume},target=/target', provenance['builder_image'],
        'cargo', 'build', '--locked', '--offline', '--release', '--target', 'x86_64-unknown-linux-gnu',
        '--manifest-path', '/bundle/upstream/rust/Cargo.toml', '--config', '/bundle/cargo-config.toml',
        '--example', 'opennovel_engine_service')
    try:
        run('start', container)
        code = int(run('wait', container))
        logs = subprocess.run([docker, 'logs', container], capture_output=True)
        (output / 'build.log').write_bytes(logs.stdout + logs.stderr)
        if code:
            raise RuntimeError('build failed; original source retained')
        binary = output / 'opennovel_engine_service'
        run('cp', container + ':/target/x86_64-unknown-linux-gnu/release/examples/opennovel_engine_service', str(binary))
        print(json.dumps({'output': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                          'build_volume': volume}))
    finally:
        state = json.loads(run('inspect', container))[0]['State']
        if state['Running']:
            run('stop', '--time', '3', container)
        run('rm', container)


if __name__ == '__main__':
    main()
