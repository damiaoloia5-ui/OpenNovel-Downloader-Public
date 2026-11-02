"""Optional desktop stdio producer. No database, exports, arbitrary URLs or credentials."""
import json
import concurrent.futures
import queue
import re
import subprocess
import sys
import threading
import time

from persistent_engine import CREATE_FLAGS, EngineSession, EngineStopped, original_fixture
import engine_runtime_config as sandbox
import offline_engine_bundle as offline
import native_engine as native

MAX_LINE = 2 * 1024 * 1024


class PrefetchedSession:
    """Three approved chapters at most; results still leave in requested order."""
    WIDTH = 3

    def __init__(self, book, items, *, factory, cancelled):
        if (not re.fullmatch('[1-9][0-9]{9,23}', book) or not 1 <= len(items) <= 10000
                or len(set(items)) != len(items)
                or any(not re.fullmatch('[1-9][0-9]{9,23}', item) for item in items)):
            raise ValueError('bounded unique identities required')
        self.items, self.positions = list(items), {item: i for i, item in enumerate(items)}
        self.external, self.stopped = cancelled, threading.Event()
        self.sessions, self.pending = [], {}
        self.available = queue.Queue()
        self.failure, self.failure_lock = None, threading.Lock()
        self.executor = None
        owner = self

        class Stop:
            def is_set(self):
                return owner.external.is_set() or owner.stopped.is_set()

        self.cancelled = Stop()
        try:
            for _ in range(min(self.WIDTH, len(items))):
                session = factory(book, items, live=True, cancelled=self.cancelled)
                self.sessions.append(session)
                self.available.put(session)
            self.executor = concurrent.futures.ThreadPoolExecutor(max_workers=len(self.sessions))
        except BaseException:
            self.close()
            raise

    def _check(self):
        with self.failure_lock:
            failure = self.failure
        if failure is not None:
            raise failure
        if self.cancelled.is_set():
            raise EngineStopped('prefetch cancelled')

    def _fetch(self, item):
        session = self.available.get()
        try:
            self._check()
            return session.request(item, cancelled=self.cancelled)
        except BaseException as error:
            with self.failure_lock:
                if self.failure is None:
                    self.failure = error
            self.stopped.set()
            raise
        finally:
            self.available.put(session)

    def _wait(self, future):
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            self._check()
            try:
                return future.result(timeout=0.03)
            except concurrent.futures.TimeoutError:
                continue
        raise EngineStopped('prefetch response timeout')

    def request(self, item, *, cancelled):
        self._check()
        if cancelled.is_set() or item not in self.positions:
            raise EngineStopped('unapproved or cancelled chapter')
        index = self.positions[item]
        desired = self.items[index:index + self.WIDTH]
        # A resumed queue may skip cached entries. Drain bounded old work before
        # scheduling a new window; never accumulate unrequested bodies.
        for old, future in list(self.pending.items()):
            if old not in desired:
                if not future.cancel():
                    self._wait(future)
                del self.pending[old]
        for upcoming in desired:
            self._check()
            if upcoming not in self.pending:
                self.pending[upcoming] = self.executor.submit(self._fetch, upcoming)
        result = self._wait(self.pending.pop(item))
        self._check()
        return result

    def close(self):
        self.stopped.set()
        for future in self.pending.values():
            future.cancel()
        if self.executor is not None:
            self.executor.shutdown(wait=True, cancel_futures=True)
        # Requests observe stopped and may close their own native child. Join
        # first so two threads never close the same Windows process handles.
        for session in self.sessions:
            session.close()
        self.pending.clear()


def emit(value):
    sys.stdout.write(json.dumps(value, ensure_ascii=False) + '\n')
    sys.stdout.flush()


def check():
    try:
        sandbox.verify_source()
    except Exception:
        return 'source_changed'
    if native.present():
        return native.check()
    try:
        result = subprocess.run([sandbox.DOCKER, 'info', '--format', '{{.OSType}}'], check=True,
                                capture_output=True, text=True, timeout=5, creationflags=CREATE_FLAGS)
        if result.stdout.strip() == 'windows':
            return 'docker_windows_mode'
        if result.stdout.strip() != 'linux':
            return 'docker_unavailable'
    except FileNotFoundError:
        return 'docker_missing'
    except (OSError, subprocess.SubprocessError):
        return 'docker_unavailable'
    if offline.present():
        try:
            lock = offline.verify()
            if not offline.image_ready(lock):
                return 'offline_bundle_pending'
            subprocess.run([sandbox.DOCKER, 'run', '--rm', '--pull=never', '--network=none', '--read-only',
                '--user=10001:10001', '--cap-drop=ALL', '--security-opt=no-new-privileges', '--log-driver=none',
                lock['image_id'], 'test', '-x', lock['binary_path']], check=True,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=8, creationflags=CREATE_FLAGS)
            return None
        except offline.BundleError:
            return 'offline_bundle_invalid'
        except (OSError, subprocess.SubprocessError):
            return 'engine_missing'
    for args in [('image', 'inspect', sandbox.RUST_IMAGE),
                 ('image', 'inspect', 'python:3.12-slim'), ('volume', 'inspect', sandbox.TARGET)]:
        try:
            subprocess.run([sandbox.DOCKER, *args], check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=5, creationflags=CREATE_FLAGS)
        except (OSError, subprocess.SubprocessError):
            return 'engine_missing' if args[0] == 'volume' else 'images_missing'
    try:
        subprocess.run([sandbox.DOCKER, 'run', '--rm', '--pull=never', '--network=none', '--read-only',
        '--user=10001:10001', '--cap-drop=ALL', '--security-opt=no-new-privileges',
        '--log-driver=none', '--mount', f'type=volume,source={sandbox.TARGET},target=/target,readonly',
        sandbox.RUST_IMAGE, 'test', '-x',
        '/target/x86_64-unknown-linux-gnu/debug/examples/opennovel_engine_service'],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=8,
        creationflags=CREATE_FLAGS)
    except (OSError, subprocess.SubprocessError):
        return 'engine_missing'
    return None


def serve(fixture=False):
    stopped, messages = threading.Event(), queue.Queue(maxsize=1)

    def read():
        try:
            for _ in range(10002):
                raw = sys.stdin.buffer.readline(MAX_LINE + 1)
                if not raw or len(raw) > MAX_LINE or not raw.endswith(b'\n'):
                    break
                value = json.loads(raw)
                if value == {'op': 'cancel'}:
                    break
                messages.put_nowait(value)
            else:
                return
        except (ValueError, queue.Full):
            pass
        finally:
            stopped.set()

    threading.Thread(target=read, daemon=True).start()
    session = None
    try:
        config = messages.get(timeout=5)
        if set(config) != {'op', 'book_id', 'items'} or config['op'] != 'configure':
            raise ValueError('configuration refused')
        if fixture and (config['book_id'] != '10000000001' or any(x not in ['10000000002', '10000000003'] for x in config['items'])):
            raise ValueError('fixture identities refused')
        factory = native.NativeSession if native.present() else EngineSession
        if native.present() and not fixture:
            session = PrefetchedSession(config['book_id'], config['items'], factory=factory, cancelled=stopped)
        else:
            session = factory(config['book_id'], config['items'], live=not fixture,
                                    fixture=original_fixture if fixture else None, cancelled=stopped)
        if stopped.is_set():
            return
        emit({'op': 'ready'})
        sequence = 0
        while not stopped.is_set():
            try:
                message = messages.get(timeout=0.1)
            except queue.Empty:
                continue
            if set(message) != {'op', 'item_id', 'sequence'} or message['op'] != 'read' or type(message['sequence']) is not int or message['sequence'] != sequence + 1:
                raise ValueError('request refused')
            sequence += 1
            result = session.request(message['item_id'], cancelled=stopped)
            if not stopped.is_set():
                emit({'op': 'chapter', 'sequence': sequence, 'envelope': result})
    except EngineStopped as error:
        if not stopped.is_set():
            emit({'op': 'error', 'kind': error.kind, **error.details})
    except Exception:
        if not stopped.is_set():
            emit({'op': 'error', 'kind': 'stopped'})
    finally:
        stopped.set()
        if session is not None:
            session.close()


def prepare():
    """Explicit action only: start installed Desktop and import a verified local image."""
    reason = check()
    if reason == 'docker_unavailable':
        try:
            subprocess.run([sandbox.DOCKER, 'desktop', 'start', '--detach'], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=8,
                           creationflags=CREATE_FLAGS)
        except (OSError, subprocess.SubprocessError):
            return 'docker_unavailable'
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            reason = check()
            if reason != 'docker_unavailable':
                break
            time.sleep(1)
    if reason == 'offline_bundle_pending':
        try:
            offline.install()
        except offline.BundleError as error:
            return error.reason
        return check()
    return reason


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    if sys.argv[1:] in (['--check'], ['--prepare']):
        try:
            reason = prepare() if sys.argv[1:] == ['--prepare'] else check()
            emit({'ready': reason is None, 'reason': reason})
        except Exception:
            emit({'ready': False})
    elif sys.argv[1:] in ([], ['--fixture']):
        serve(fixture=bool(sys.argv[1:]))
    else:
        sys.exit(2)
