"""Private persistent engine session. Research API, not enabled by the desktop app."""
import base64
import json
import os
import queue
import re
import secrets
import subprocess
import threading
import time

import run_local_engine_gate as sandbox
from https_read_gate import fetch_once, validate_request


class EngineStopped(RuntimeError):
    pass


class EngineSession:
    def __init__(self, book_id, items, *, live=False, fixture=None):
        if not re.fullmatch('[1-9][0-9]{9,23}', book_id) or not items or len(items) > 10000 or any(not re.fullmatch('[1-9][0-9]{9,23}', x) for x in items):
            raise ValueError('explicit bounded identities required')
        if not live and fixture is None:
            raise ValueError('controlled fixture required when offline')
        self.book_id, self.items, self.live, self.fixture = book_id, frozenset(items), live, fixture
        self.sequence, self.requests = 0, 0
        self._closed, self._lock = threading.Event(), threading.Lock()
        self._containers, self.evidence = [], []
        self._child = None
        self._responses = queue.Queue(maxsize=1)
        self.env = dict(os.environ, OPENNOVEL_REVIEW_CAPABILITY=secrets.token_hex(32))
        sandbox.verify_source()
        try:
            python = self._docker('image', 'inspect', 'python:3.12-slim', '--format', '{{.Id}}')
            self.relay = self._create(python, ['python', '-u', '/review/persistent_read_gate.py'], 'none',
                [f'type=bind,source={sandbox.SCRIPTS},target=/review,readonly'])
            self._docker('start', self.relay)
            self.engine = self._create(sandbox.RUST_IMAGE,
                ['/target/x86_64-unknown-linux-gnu/debug/examples/opennovel_engine_service'],
                'container:' + self.relay, [f'type=volume,source={sandbox.TARGET},target=/target,readonly'])
            self._child = subprocess.Popen(['docker', 'start', '-ai', self.engine], env=self.env,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            self._reader = threading.Thread(target=self._read_responses, daemon=True)
            self._reader.start()
        except BaseException:
            self.close()
            raise

    def _docker(self, *args, input=None):
        result = subprocess.run(['docker', *args], input=input, text=True, capture_output=True,
            encoding='utf-8', errors='replace', timeout=15, env=self.env)
        if result.returncode:
            raise EngineStopped('isolated engine operation failed')
        return result.stdout.strip()

    def _create(self, image, args, network, mounts):
        command = ['create', '-i', '--network', network, '--read-only', '--user', '10001:10001',
            '--log-driver=none', '--cap-drop=ALL', '--security-opt=no-new-privileges',
            '--pids-limit=256', '--memory=2g', '--cpus=2', '--env', 'OPENNOVEL_REVIEW_CAPABILITY',
            '--env', 'PYTHONDONTWRITEBYTECODE=1', '--tmpfs', '/tmp:rw,nosuid,nodev,size=134217728']
        for mount in mounts:
            command += ['--mount', mount]
        identifier = self._docker(*command, image, *args)
        self._containers.append(identifier)
        inspect = json.loads(self._docker('inspect', identifier))[0]
        host = inspect['HostConfig']
        if host['NetworkMode'] != network or not host['ReadonlyRootfs'] or host['PortBindings'] or host['Privileged'] or host['CapAdd'] or host['CapDrop'] != ['ALL'] or host['LogConfig']['Type'] != 'none' or inspect['Config']['User'] != '10001:10001':
            raise EngineStopped('isolation refused')
        self.evidence.append({'container': identifier, 'network': network, 'user': '10001:10001',
                              'mounts': [{'path': m['Destination'], 'rw': m['RW']} for m in inspect['Mounts']]})
        return identifier

    def _read_responses(self):
        try:
            while not self._closed.is_set():
                line = self._child.stdout.readline(2 * 1024 * 1024 + 1)
                if not line or len(line) > 2 * 1024 * 1024 or not line.endswith(b'\n'):
                    raise EngineStopped('engine output closed or oversized')
                self._responses.put(json.loads(line), timeout=1)
        except (OSError, ValueError, queue.Full, EngineStopped):
            try:
                self._responses.put_nowait(None)
            except queue.Full:
                pass

    def _write_mail(self, name, value):
        # Names come only from this session's counter; no user path reaches Docker exec code.
        self._docker('exec', '-i', self.relay, 'python', '-c',
            f'import pathlib,sys; p=pathlib.Path("/tmp/opennovel-session/{name}.tmp"); p.write_text(sys.stdin.read()); p.rename("/tmp/opennovel-session/{name}.json")',
            input=json.dumps(value))

    def _check_stop(self, cancelled):
        if self._closed.is_set() or (cancelled is not None and cancelled.is_set()):
            raise EngineStopped('session cancelled or closed')
        if self._child is not None and self._child.poll() is not None:
            raise EngineStopped('engine process exited; create a fresh session')

    def request(self, item_id, *, cancelled=None):
        if item_id not in self.items:
            raise ValueError('chapter not approved')
        if not self._lock.acquire(blocking=False):
            raise EngineStopped('one active request per session')
        try:
            self._check_stop(cancelled)
            self.sequence += 1
            sequence = self.sequence
            self._write_mail('approval', {'sequence': sequence, 'item_id': item_id})
            self._child.stdin.write((json.dumps({'sequence': sequence, 'item_id': item_id}) + '\n').encode())
            self._child.stdin.flush()
            deadline, request = time.monotonic() + 15, None
            while time.monotonic() < deadline:
                self._check_stop(cancelled)
                raw = self._docker('exec', self.relay, 'python', '-c',
                    f'import pathlib; p=pathlib.Path("/tmp/opennovel-session/request-{sequence}.json"); print(p.read_text() if p.exists() else "")')
                if raw:
                    request = json.loads(raw)
                    break
                time.sleep(0.03)
            if request is None:
                raise EngineStopped('engine request deadline exceeded')
            validate_request(request, item_id)
            self.requests += 1
            # A daemon transfer thread cannot deliver a late response into another sequence.
            delivered = queue.Queue(maxsize=1)
            def transfer():
                try:
                    if self._closed.is_set() or (cancelled is not None and cancelled.is_set()):
                        delivered.put_nowait({'status': 503, 'headers': [], 'body': ''})
                        return
                    result = fetch_once(request, item_id)[0] if self.live else self.fixture(item_id)
                    delivered.put_nowait(result)
                except Exception:
                    delivered.put_nowait({'status': 502, 'headers': [], 'body': ''})
            threading.Thread(target=transfer, daemon=True).start()
            deadline = time.monotonic() + 20
            while True:
                self._check_stop(cancelled)
                if time.monotonic() >= deadline:
                    raise EngineStopped('transfer deadline exceeded')
                try:
                    result = delivered.get(timeout=0.03)
                    break
                except queue.Empty:
                    continue
            if result['status'] != 200:
                raise EngineStopped('upstream restricted or failed; session stopped')
            self._check_stop(cancelled)
            self._write_mail(f'response-{sequence}', result)
            deadline = time.monotonic() + 5
            while True:
                self._check_stop(cancelled)
                if time.monotonic() >= deadline:
                    raise EngineStopped('response deadline exceeded')
                try:
                    value = self._responses.get(timeout=0.03)
                    break
                except queue.Empty:
                    continue
            if not value or value.get('sequence') != sequence or value.get('ok') is not True:
                raise EngineStopped('stale or failed engine response')
            envelope = value.get('envelope', {})
            if envelope.get('book_id') != self.book_id or envelope.get('item_id') != item_id or type(envelope.get('need_pay')) is not int or envelope['need_pay'] != 0:
                raise EngineStopped('identity or permission changed')
            self._check_stop(cancelled)
            return envelope
        except BaseException as error:
            self.close()
            if isinstance(error, (OSError, ValueError, KeyError, TypeError)):
                raise EngineStopped('bounded engine communication failed') from None
            raise
        finally:
            self._lock.release()

    def close(self):
        self._closed.set()
        failures = []
        for identifier in reversed(self._containers):
            try:
                state = json.loads(self._docker('inspect', identifier))[0]['State']
                if state['Running']:
                    self._docker('stop', '--time', '1', identifier)
                self._docker('rm', identifier)
            except EngineStopped:
                failures.append(identifier)
        self._containers = failures
        if self._child is not None:
            if hasattr(self, '_reader'):
                self._reader.join(timeout=2)
            try:
                self._child.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                self._child.terminate()
                self._child.wait(timeout=3)
        if failures:
            raise EngineStopped('owned containers require cleanup')

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


def original_fixture(item):
    payload = {'code': 0, 'data': {'item_id': item, 'content': '<p>山间的小屋亮起一盏灯。</p>',
        'novel_data': {'book_id': '10000000001', 'chapter_title': '原创章', 'need_pay': '0', 'word_number': '11'}}}
    return {'status': 200, 'headers': [['content-type', 'application/json']],
            'body': base64.b64encode(json.dumps(payload, ensure_ascii=False).encode()).decode()}
