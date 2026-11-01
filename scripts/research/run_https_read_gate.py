"""Controlled relay checks by default; --live-once performs exactly one approved HTTPS GET."""
import base64
import hashlib
import json
import os
import pathlib
import secrets
import subprocess
import sys
import time
import uuid

import run_local_engine_gate as offline
from https_read_gate import fetch_once, validate_request
from local_engine_gate import ITEM, TEXT

LIVE_ITEM = '7672702368512754238'  # Previously authorized public free-platform sample.
OUTPUT = offline.REPO / '.artifacts/https-read-gate-20261101'
MAIL = '/tmp/opennovel-mail'
METADATA_FIELDS = {'outcome', 'code', 'content_characters', 'identity_matches', 'has_novel_data',
    'book_identity_matches', 'nested_identity_matches', 'canonical_characters', 'canonical_sha256',
    'declared_characters', 'novel_metadata', 'need_pay', 'is_locked', 'device_count', 'pool_file_exists'}


def split_engine_output(raw):
    value = json.loads(raw)
    envelope = value.pop('envelope', None)
    if set(value) != METADATA_FIELDS:
        raise ValueError('unexpected engine output schema')
    return value, envelope


def main(live=False, accept=False):
    OUTPUT.mkdir(parents=True, exist_ok=True)
    offline.verify_source()
    item = LIVE_ITEM if live else ITEM
    python_image = offline.docker('image', 'inspect', 'python:3.12-slim', '--format', '{{.Id}}')
    env = dict(os.environ, OPENNOVEL_REVIEW_CAPABILITY=secrets.token_hex(32), OPENNOVEL_APPROVED_ITEM=item)
    if accept:
        env['OPENNOVEL_ACCEPT_ENVELOPE'] = '1'
    else:
        env.pop('OPENNOVEL_ACCEPT_ENVELOPE', None)
    created, evidence = [], []

    def command(*args, input=None, timeout=40):
        result = subprocess.run(['docker', *args], input=input, text=True, capture_output=True,
                                encoding='utf-8', errors='replace', timeout=timeout, env=env)
        if result.returncode:
            raise RuntimeError('controlled Docker operation failed')
        return result.stdout.strip()

    def create(image, args, network, mounts):
        container = command('create', '-i', '--network', network, '--read-only', '--user', '10001:10001',
            '--log-driver=none',
            '--cap-drop=ALL', '--security-opt=no-new-privileges', '--pids-limit=256', '--memory=2g', '--cpus=2',
            '--tmpfs', '/tmp:rw,nosuid,nodev,size=134217728', '--env', 'OPENNOVEL_REVIEW_CAPABILITY',
            '--env', 'OPENNOVEL_APPROVED_ITEM', '--env', 'OPENNOVEL_ACCEPT_ENVELOPE',
            '--env', 'PYTHONDONTWRITEBYTECODE=1', *mounts, image, *args)
        created.append(container)
        inspect = json.loads(command('inspect', container))[0]
        host = inspect['HostConfig']
        if host['NetworkMode'] != network or not host['ReadonlyRootfs'] or host['PortBindings'] or host['Privileged'] or host['CapAdd'] or host['CapDrop'] != ['ALL'] or host['LogConfig']['Type'] != 'none':
            raise RuntimeError('isolation refused')
        evidence.append({'network': network, 'user': inspect['Config']['User'], 'read_only_root': True,
                         'mounts': [{'destination': x['Destination'], 'rw': x['RW']} for x in inspect['Mounts']]})
        return container

    report = {'live': live, 'approved_requests': 0, 'accepted_for_download': False}
    child = None
    try:
        relay = create(python_image, ['python', '-u', '/review/relay_read_gate.py'], 'none',
                       ['--mount', f'type=bind,source={offline.SCRIPTS},target=/review,readonly'])
        command('start', relay)
        engine = create(offline.RUST_IMAGE, ['/target/x86_64-unknown-linux-gnu/debug/examples/opennovel_readonly_probe'],
                        'container:' + relay, ['--mount', f'type=volume,source={offline.TARGET},target=/target,readonly'])
        # Attachment supplies stdin but returns immediately to the host relay loop.
        child = subprocess.Popen(['docker', 'start', '-ai', engine], stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        child.stdin.write(json.dumps({'item_id': item}).encode())
        child.stdin.close()
        child.stdin = None
        deadline = time.monotonic() + 15
        request = None
        while time.monotonic() < deadline:
            raw = command('exec', relay, 'python', '-c',
                f'import pathlib; p=pathlib.Path("{MAIL}/request.json"); print(p.read_text() if p.exists() else "")')
            if raw:
                request = json.loads(raw)
                break
            time.sleep(0.1)
        if request is None:
            raise RuntimeError('engine did not produce a bounded request')
        validate_request(request, item)
        if live:
            response, transfer = fetch_once(request, item)
            report.update(transfer)
            report['approved_requests'] = 1
        else:
            response = {'status': 200, 'headers': [['content-type', 'application/json']],
                        'body': base64.b64encode(json.dumps({'code': 0, 'data': {'item_id': item, 'content': TEXT}}, ensure_ascii=False).encode()).decode()}
            report['outcome'] = 'controlled_relay'
        # Payloads travel only via subprocess pipes and container tmpfs, never host logs/files.
        command('exec', '-i', relay, 'python', '-c',
            f'import pathlib,sys; p=pathlib.Path("{MAIL}/response.tmp"); p.write_text(sys.stdin.read()); p.rename("{MAIL}/response.json")',
            input=json.dumps(response))
        stdout, stderr = child.communicate(timeout=30)
        exit_code = int(command('inspect', engine, '--format', '{{.State.ExitCode}}'))
        report['engine_exit'] = exit_code
        if not live:
            value = json.loads(stdout)
            if exit_code or value['sha256'] != hashlib.sha256(TEXT.encode()).hexdigest() or value['device_count']:
                raise RuntimeError('controlled relay contract failed')
            report['fixture_matches'] = True
        elif stdout:
            value, envelope = split_engine_output(stdout)
            # Only the wrapper's fixed metadata schema may reach the host report.
            report['response_metadata'] = value
            reference = json.loads((OUTPUT / 'reference-check.json').read_text(encoding='utf-8'))
            report['reference_matches'] = (value['book_identity_matches'] is True and
                value['identity_matches'] is True and value['nested_identity_matches'] is True and
                value['code'] == 0 and value['need_pay'] == 0 and value['device_count'] == 0 and
                value['pool_file_exists'] is False and value['canonical_characters'] == reference['reference_characters'] and
                value['declared_characters'] == reference['declared_characters'] and
                value['canonical_sha256'] == reference['reference_sha256'])
            if accept:
                if not report['reference_matches'] or envelope is None:
                    raise RuntimeError('reference/permission acceptance refused; no library created')
                destination = OUTPUT / ('acceptance-' + uuid.uuid4().hex)
                accepted = subprocess.run([str(offline.REPO / 'target/debug/examples/accept_engine_chapter.exe'),
                    reference['book_id'], item, reference['title'], str(destination)],
                    input=json.dumps(envelope, ensure_ascii=False), text=True, capture_output=True,
                    encoding='utf-8', timeout=20)
                if accepted.returncode:
                    raise RuntimeError('core acceptance refused')
                receipt = json.loads(accepted.stdout)
                if receipt['saved'] != 1:
                    raise RuntimeError('single chapter was not saved')
                report['acceptance_receipt'] = receipt
                report['accepted_for_download'] = True
        report['isolation'] = evidence
        offline.verify_source()
        (OUTPUT / ('live-report.json' if live else 'controlled-report.json')).write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(json.dumps(report, ensure_ascii=True), flush=True)
    finally:
        for container in reversed(created):
            inspect = json.loads(command('inspect', container))[0]
            if inspect['State']['Running']:
                command('stop', '--time', '3', container)
            command('rm', container)
        if child is not None and child.poll() is None:
            try:
                child.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                child.terminate()
                child.wait(timeout=3)


if __name__ == '__main__':
    if sys.argv[1:] not in ([], ['--live-once'], ['--accept-sample']):
        raise SystemExit('only --live-once or --accept-sample is supported')
    main(live=bool(sys.argv[1:]), accept=sys.argv[1:] == ['--accept-sample'])
