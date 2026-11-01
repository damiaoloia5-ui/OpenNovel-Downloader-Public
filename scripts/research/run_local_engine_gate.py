"""Explicit offline research runner, separate from application build and normal CI."""
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import subprocess

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = pathlib.Path(__file__).resolve().parent
PREVIOUS = REPO / '.artifacts/local-engine-review-20261101'
SOURCE = PREVIOUS / 'source'
OUTPUT = REPO / '.artifacts/local-engine-gate-20261101'
RUST_IMAGE = 'sha256:e18a79fc84dfcfc3ab5ba72290398a644c135c97eaa881447fddc354ee4701a3'
CACHE = 'opennovel-engine-review-20261101-cargo'
TARGET = 'opennovel-engine-review-20261101-target'


def docker(*args, input=None, timeout=240):
    result = subprocess.run(['docker', *args], input=input, capture_output=True, text=True,
                            encoding='utf-8', errors='replace', timeout=timeout)
    if result.returncode:
        # Avoid echoing command arguments or environment capabilities on failures.
        OUTPUT.mkdir(parents=True, exist_ok=True)
        capability = os.environ.get('OPENNOVEL_REVIEW_CAPABILITY', '')
        detail = result.stderr
        if capability:
            detail = detail.replace(capability, '[redacted]')
        (OUTPUT / 'docker-error.log').write_text(detail, encoding='utf-8')
        raise RuntimeError(f'docker {args[0]} failed with exit {result.returncode}')
    return result.stdout.strip()


def verify_source():
    manifest = json.loads((PREVIOUS / 'inventory.json').read_text(encoding='utf-8'))
    if manifest['commit'] != '07cfb2249d4fd57d19bf4420af0e97621ec92df9':
        raise RuntimeError('unapproved candidate revision')
    actual = {p.relative_to(SOURCE).as_posix() for p in SOURCE.rglob('*') if p.is_file()}
    if actual != {entry['path'] for entry in manifest['files']}:
        raise RuntimeError('candidate inventory changed')
    for entry in manifest['files']:
        if hashlib.sha256((SOURCE / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise RuntimeError('candidate bytes changed')
    return manifest


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    manifest = verify_source()
    derived = OUTPUT / 'build-source'
    if not derived.exists():
        shutil.copytree(SOURCE, derived)
        (derived / 'rust/examples').mkdir()
    for entry in manifest['files']:
        if hashlib.sha256((derived / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise RuntimeError('derived candidate bytes changed')
    if {p.relative_to(derived).as_posix() for p in derived.rglob('*') if p.is_file()} != {entry['path'] for entry in manifest['files']}:
        raise RuntimeError('unexpected files in derived candidate')
    # Reuse only installed images and pre-fetched dependency volumes. No image pull/fetch here.
    docker('image', 'inspect', RUST_IMAGE)
    python_image = docker('image', 'inspect', 'python:3.12-slim', '--format', '{{.Id}}')
    for name in (CACHE, TARGET):
        docker('volume', 'inspect', name)
    original_capability = os.environ.get('OPENNOVEL_REVIEW_CAPABILITY')
    os.environ['OPENNOVEL_REVIEW_CAPABILITY'] = secrets.token_hex(32)
    records, created = [], []

    def create(label, image, command, network='none', mounts=(), stdin=False):
        args = ['create', '--network', network, '--read-only', '--user', '10001:10001',
                '--cap-drop=ALL', '--security-opt=no-new-privileges', '--pids-limit=256',
                '--memory=2g', '--cpus=2', '--tmpfs', '/tmp:rw,nosuid,nodev,size=134217728',
                '--env', 'OPENNOVEL_REVIEW_CAPABILITY', '--env', 'PYTHONDONTWRITEBYTECODE=1']
        if stdin:
            args.append('-i')
        for mount in mounts:
            args += ['--mount', mount]
        container = docker(*args, image, *command)
        created.append(container)
        record = {'label': label, 'container_id': container}
        records.append(record)
        inspect = json.loads(docker('inspect', container))[0]
        config = inspect['HostConfig']
        assert config['NetworkMode'] == network and config['ReadonlyRootfs']
        assert config['CapDrop'] == ['ALL'] and not config['CapAdd'] and not config['Privileged']
        assert config['PortBindings'] == {} and inspect['Config']['User'] == '10001:10001'
        record.update({'image': inspect['Image'], 'user': inspect['Config']['User'],
                  'network': network, 'read_only_root': True, 'ports': {}, 'cap_drop': ['ALL'],
                  'mounts': [{'destination': m['Destination'], 'rw': m['RW']} for m in inspect['Mounts']]})
        return container, record

    def save_logs(container, label):
        logs = subprocess.run(['docker', 'logs', container], capture_output=True, text=True,
                              encoding='utf-8', errors='replace', timeout=20)
        text = (logs.stdout + logs.stderr).replace(os.environ['OPENNOVEL_REVIEW_CAPABILITY'], '[redacted]')
        (OUTPUT / f'{label}.log').write_text(text, encoding='utf-8')

    try:
        build, record = create('build', RUST_IMAGE,
            ['env', 'CARGO_HOME=/cargo', 'CARGO_TARGET_DIR=/target', 'CARGO_INCREMENTAL=0',
             'RUSTUP_HOME=/usr/local/rustup', 'cargo', 'build', '--manifest-path', '/src/rust/Cargo.toml',
             '--locked', '--offline', '--target', 'x86_64-unknown-linux-gnu', '--example', 'opennovel_readonly_probe'],
            mounts=[f'type=bind,source={derived},target=/src,readonly',
                    f'type=bind,source={SCRIPTS / "engine_probe_examples"},target=/src/rust/examples,readonly',
                    f'type=volume,source={CACHE},target=/cargo', f'type=volume,source={TARGET},target=/target'])
        docker('start', build)
        record['exit_code'] = int(docker('wait', build, timeout=600))
        save_logs(build, 'build')
        if record['exit_code']:
            raise RuntimeError('offline harness build failed; see build.log')
        print('offline harness build passed', flush=True)

        broker, _ = create('broker', python_image, ['python', '-u', '/review/local_engine_gate.py'],
                           mounts=[f'type=bind,source={SCRIPTS},target=/review,readonly'])
        docker('start', broker)
        checks = docker('exec', broker, 'python', '/review/local_engine_gate_checks.py')
        check_report = json.loads(checks)
        (OUTPUT / 'http-checks.json').write_text(json.dumps(check_report, indent=2), encoding='utf-8')
        print('broker HTTP checks passed', flush=True)
        for label, request, expected in [
            ('chapter', {'item_id': '10000000002'}, 0),
            ('wrong-identity', {'item_id': '10000000009'}, 3),
            ('arbitrary-operation', {'item_id': '10000000002', 'url': 'https://example.invalid/'}, 3),
        ]:
            engine, record = create(label, RUST_IMAGE,
                ['/target/x86_64-unknown-linux-gnu/debug/examples/opennovel_readonly_probe'],
                network='container:' + broker,
                mounts=[f'type=volume,source={TARGET},target=/target,readonly'], stdin=True)
            attached = subprocess.run(['docker', 'start', '-ai', engine], input=json.dumps(request),
                                      capture_output=True, text=True, encoding='utf-8', timeout=20)
            record['exit_code'] = int(docker('inspect', engine, '--format', '{{.State.ExitCode}}'))
            save_logs(engine, label)
            assert record['exit_code'] == expected
            if expected == 0:
                value = json.loads(attached.stdout)
                assert value['outcome'] == 'controlled_fixture' and value['device_count'] == 0
                assert value['sha256'] == check_report['fixture_sha256']
            print(label, 'expected exit', expected, flush=True)
        verify_source()
        save_logs(broker, 'broker')
        events = [json.loads(line) for line in (OUTPUT / 'broker.log').read_text(encoding='utf-8').splitlines()]
        requests = [event for event in events if 'status' in event]
        assert len(requests) == check_report['http_cases_passed'] + 1
        assert sum(event['status'] == 200 for event in requests) == 2
        # No targets, headers, capability or content may be persisted by the broker.
        assert all(set(event) == {'status', 'outcome'} for event in requests)
        report = {'source_commit': manifest['commit'], 'source_unchanged_files': len(manifest['files']),
                  'fixture_only': True, 'real_platform_requested': False,
                  'http_cases_passed': check_report['http_cases_passed'],
                  'broker_request_count': len(requests),
                  'review_script_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in SCRIPTS.glob('*.py')},
                  'harness_sha256': hashlib.sha256((SCRIPTS / 'engine_probe_examples/opennovel_readonly_probe.rs').read_bytes()).hexdigest(),
                  'containers': records}
        (OUTPUT / 'evidence.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    finally:
        # Only exact IDs created in this invocation; no recursive filesystem or global Docker cleanup.
        for container in reversed(created):
            inspect = json.loads(docker('inspect', container))[0]
            if inspect['State']['Running']:
                docker('stop', '--time', '3', container)
            save_logs(container, next(x['label'] for x in records if x['container_id'] == container))
            docker('rm', container)
        if original_capability is None:
            os.environ.pop('OPENNOVEL_REVIEW_CAPABILITY', None)
        else:
            os.environ['OPENNOVEL_REVIEW_CAPABILITY'] = original_capability


if __name__ == '__main__':
    main()
