"""Optional desktop stdio producer. No database, exports, arbitrary URLs or credentials."""
import json
import queue
import subprocess
import sys
import threading
import time

from persistent_engine import CREATE_FLAGS, EngineSession, EngineStopped, original_fixture
import engine_runtime_config as sandbox
import offline_engine_bundle as offline

MAX_LINE = 2 * 1024 * 1024


def emit(value):
    sys.stdout.write(json.dumps(value, ensure_ascii=False) + '\n')
    sys.stdout.flush()


def check():
    try:
        sandbox.verify_source()
    except Exception:
        return 'source_changed'
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
        session = EngineSession(config['book_id'], config['items'], live=not fixture,
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
