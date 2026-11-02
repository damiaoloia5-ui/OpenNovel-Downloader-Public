"""Bounded stdio broker for the independently packaged Windows GPL engine."""
import hashlib
import json
import pathlib
import queue
import re
import threading
import time

import engine_runtime_config as config
from https_read_gate import ChapterTransport, fetch_with_retries, validate_request
from persistent_engine import EngineStopped, original_fixture
from windows_engine_sandbox import IsolatedProcess

MAX_LINE = 3 * 1024 * 1024


def present():
    return (config.PREVIOUS / 'native-engine').exists()


def verify():
    directory = config.PREVIOUS / 'native-engine'
    raw = (pathlib.Path(__file__).parent / 'native_engine_lock.json').read_bytes()
    lock = json.loads(raw)
    if (directory / 'manifest.json').read_bytes() != raw:
        raise ValueError('native manifest changed')
    expected = {'engine.exe', 'corresponding-source.tar.gz'}
    if set(lock['files']) != expected or {p.name for p in directory.iterdir()} != expected | {'manifest.json'}:
        raise ValueError('native inventory changed')
    for name, entry in lock['files'].items():
        path = directory / name
        if not path.is_file() or path.is_symlink() or path.stat().st_size != entry['bytes']:
            raise ValueError('native file changed')
        if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('native digest changed')
    return directory / 'engine.exe', lock['files']['engine.exe']['sha256']


class NativeSession:
    def __init__(self, book_id, items, *, live=False, fixture=None, cancelled=None):
        if not re.fullmatch('[1-9][0-9]{9,23}', book_id) or not items or len(items) > 10000 or any(not re.fullmatch('[1-9][0-9]{9,23}', item) for item in items):
            raise ValueError('bounded identities required')
        if not live and fixture is None:
            raise ValueError('explicit offline fixture required')
        self.book_id, self.items, self.live, self.fixture = book_id, frozenset(items), live, fixture
        self.sequence, self.requests = 0, 0
        self._closed, self._lock = threading.Event(), threading.Lock()
        self._responses = queue.Queue(maxsize=2)
        self._child = None
        self._reader = None
        self._transport = ChapterTransport()
        self._check_stop(cancelled)
        binary, digest = verify()
        try:
            self._child = IsolatedProcess(binary, expected_sha256=digest)
            self._reader = threading.Thread(target=self._read_responses, daemon=True)
            self._reader.start()
            self._check_stop(cancelled)
        except BaseException:
            self.close()
            raise

    def _read_responses(self):
        try:
            while not self._closed.is_set():
                raw = self._child.stdout.readline(MAX_LINE + 1)
                if not raw or len(raw) > MAX_LINE or not raw.endswith(b'\n'):
                    break
                self._responses.put(json.loads(raw), timeout=0.1)
        except (OSError, ValueError, queue.Full):
            pass
        finally:
            try:
                self._responses.put_nowait(None)
            except queue.Full:
                pass

    def _check_stop(self, cancelled):
        if self._closed.is_set() or (cancelled is not None and cancelled.is_set()):
            raise EngineStopped('native session cancelled')
        if self._child is not None and self._child.poll() is not None:
            raise EngineStopped('native engine exited; fresh session required')

    def _wait(self, messages, cancelled, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self._check_stop(cancelled)
            try:
                value = messages.get(timeout=0.03)
                if value is None:
                    raise EngineStopped('native stream failed')
                return value
            except queue.Empty:
                pass
        raise EngineStopped('native response deadline exceeded')

    def _send(self, value, cancelled):
        raw = (json.dumps(value, ensure_ascii=False) + '\n').encode()
        if len(raw) > MAX_LINE:
            raise EngineStopped('native frame too large')
        done = queue.Queue(maxsize=1)
        def write():
            try:
                offset = 0
                while offset < len(raw):
                    written = self._child.stdin.write(raw[offset:])
                    if not written:
                        raise OSError('closed pipe')
                    offset += written
                done.put_nowait(True)
            except (OSError, ValueError, AttributeError):
                done.put_nowait(None)
        threading.Thread(target=write, daemon=True).start()
        self._wait(done, cancelled, 5)

    def request(self, item_id, *, cancelled=None):
        if item_id not in self.items:
            raise ValueError('chapter not approved')
        if not self._lock.acquire(blocking=False):
            raise EngineStopped('one active request per session')
        try:
            self._check_stop(cancelled)
            self.sequence += 1
            self._send({'sequence': self.sequence, 'item_id': item_id}, cancelled)
            request = self._wait(self._responses, cancelled, 5)
            if set(request) != {'op', 'sequence', 'request'} or request['op'] != 'fetch' or type(request['sequence']) is not int or request['sequence'] != self.sequence:
                raise EngineStopped('unapproved native request')
            validate_request(request['request'], item_id)
            self.requests += 1
            delivered = queue.Queue(maxsize=1)
            def transfer():
                try:
                    stop = lambda: self._closed.is_set() or (cancelled is not None and cancelled.is_set())
                    if stop():
                        delivered.put_nowait(None)
                        return
                    result = fetch_with_retries(request['request'], item_id, stop_requested=stop, transport=self._transport) if self.live else (self.fixture(item_id), {})
                    delivered.put_nowait(result)
                except Exception:
                    delivered.put_nowait(({'status': 502}, {'outcome': 'transport_failed'}))
            threading.Thread(target=transfer, daemon=True).start()
            result, report = self._wait(delivered, cancelled, 20)
            if result['status'] != 200:
                kind = 'rate_limited' if report.get('outcome') == 'rate_limited' or result['status'] == 429 else 'access_challenge' if result['status'] in (401, 403) else 'stopped'
                raise EngineStopped('upstream stopped', kind, report)
            self._send(dict(result, sequence=self.sequence), cancelled)
            value = self._wait(self._responses, cancelled, 5)
            if value.get('sequence') != self.sequence or value.get('ok') is not True:
                raise EngineStopped('stale or failed native response')
            envelope = value.get('envelope', {})
            if envelope.get('book_id') != self.book_id or envelope.get('item_id') != item_id or type(envelope.get('need_pay')) is not int or envelope['need_pay'] != 0:
                raise EngineStopped('identity or permission changed', 'permission_changed')
            self._check_stop(cancelled)
            return envelope
        except BaseException as error:
            self.close()
            if isinstance(error, (OSError, ValueError, KeyError, TypeError)):
                raise EngineStopped('bounded native communication failed') from None
            raise
        finally:
            self._lock.release()

    def close(self):
        self._closed.set()
        self._transport.close()
        if self._child is not None:
            self._child.close()
        if self._reader is not None:
            self._reader.join(timeout=2)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def check():
    try:
        verify()
    except (OSError, ValueError, KeyError):
        return 'native_bundle_invalid'
    try:
        with NativeSession('10000000001', ['10000000002'], fixture=original_fixture) as session:
            value = session.request('10000000002')
            if value['content'] != '<p>山间的小屋亮起一盏灯。</p>':
                return 'native_sandbox_failed'
        return None
    except Exception:
        return 'native_sandbox_failed'
