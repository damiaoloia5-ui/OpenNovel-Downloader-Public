"""Explicit Windows native acceptance using ONLY original fixtures; no platform or user library."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
import pathlib
import queue
import socket
import subprocess
import sys
import threading
import time
import tempfile
from unittest.mock import patch


def alive(pid):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    handle = kernel.OpenProcess(0x100000, False, pid)
    if not handle:
        return False
    try:
        return kernel.WaitForSingleObject(handle, 0) == 258
    finally:
        kernel.CloseHandle(handle)


def run(component, output):
    if os.name != 'nt' or not component.is_absolute() or output.exists():
        raise ValueError('Windows, absolute component and new evidence directory required')
    output.mkdir(parents=True)
    os.environ['OPENNOVEL_ENGINE_ROOT'] = str(component)
    import native_engine as native
    from persistent_engine import EngineStopped, original_fixture
    from windows_engine_sandbox import IsolatedProcess
    report = {'passed': False, 'real_platform_requested': False, 'user_library_opened': False, 'checks': {}}
    checks = report['checks']
    try:
        assert native.check() is None
        checks['original_startup'] = True
        start = time.monotonic()
        with native.NativeSession('10000000001', ['10000000002'], fixture=original_fixture) as session:
            pid = session._child.pid
            for _ in range(1000):
                assert session.request('10000000002')['content'] == '<p>山间的小屋亮起一盏灯。</p>'
                assert session._child.pid == pid
            assert session.sequence == session.requests == 1000
        checks['1000_original_chapters_one_process_seconds'] = round(time.monotonic() - start, 3)
        assert not alive(pid)
        checks['normal_exit_cleanup'] = True

        for status, outcome, kind in [(403, 'access_challenge', 'access_challenge'), (429, 'rate_limited', 'rate_limited')]:
            with patch.object(native, 'fetch_with_retries', return_value=({'status': status}, {'outcome': outcome, 'retry_after_seconds': 60})) as fetch:
                session = native.NativeSession('10000000001', ['10000000002'], live=True)
                pid = session._child.pid
                try:
                    session.request('10000000002')
                    raise AssertionError('restriction accepted')
                except EngineStopped as error:
                    assert error.kind == kind
                assert fetch.call_count == 1 and not alive(pid)
        checks['challenge_and_rate_limit_stop_without_retry'] = True

        entered, release, cancelled = threading.Event(), threading.Event(), threading.Event()
        def delayed(item):
            entered.set(); release.wait(5)
            return original_fixture(item)
        session = native.NativeSession('10000000001', ['10000000002'], fixture=delayed)
        pid, results = session._child.pid, queue.Queue()
        def read():
            try: results.put(session.request('10000000002', cancelled=cancelled))
            except EngineStopped: results.put('cancelled')
        thread = threading.Thread(target=read); thread.start()
        assert entered.wait(5)
        cancelled.set(); thread.join(5); release.set()
        assert results.get(timeout=1) == 'cancelled' and not thread.is_alive() and not alive(pid)
        with native.NativeSession('10000000001', ['10000000002'], fixture=original_fixture) as restarted:
            assert restarted.request('10000000002')['item_id'] == '10000000002'
        checks['cancel_discards_late_result_and_fresh_session_recovers'] = True

        for field, replacement in [('need_pay', '1'), ('book_id', '10000000009')]:
            def wrong_metadata(item):
                import base64
                value = original_fixture(item)
                payload = json.loads(base64.b64decode(value['body']))
                payload['data']['novel_data'][field] = replacement
                value['body'] = base64.b64encode(json.dumps(payload).encode()).decode()
                return value
            session = native.NativeSession('10000000001', ['10000000002'], fixture=wrong_metadata)
            pid = session._child.pid
            try:
                session.request('10000000002')
                raise AssertionError('changed permission or identity accepted')
            except EngineStopped:
                pass
            finally:
                session.close()
            assert not alive(pid)
        checks['paid_or_wrong_book_body_rejected'] = True

        probe = output / 'sandbox-probe.exe'
        subprocess.run(['rustc', '--edition=2021', '-C', 'target-feature=+crt-static', str(pathlib.Path(__file__).with_name('native_sandbox_probe.rs')), '-o', str(probe)], check=True)
        private = output / 'private-original.txt'; private.write_text('original private fixture')
        with socket.socket() as server:
            server.bind(('127.0.0.1', 0)); server.listen()
            isolated = IsolatedProcess(probe, expected_sha256=hashlib.sha256(probe.read_bytes()).hexdigest())
            try:
                for command, expected in [('read:' + str(private), 'read_denied:true'), ('write:' + str(private), 'write_denied:true'), ('connect:127.0.0.1:' + str(server.getsockname()[1]), 'network_denied:true'), ('spawn', 'spawn_denied:true')]:
                    isolated.stdin.write((command + '\n').encode())
                    assert isolated.stdout.readline().decode().strip() == expected
            finally:
                isolated.close()
        assert private.read_text() == 'original private fixture'
        checks['file_network_child_process_denied'] = True

        owner_code = '''import sys,pathlib,hashlib,json,time
sys.path.insert(0,sys.argv[1])
from windows_engine_sandbox import IsolatedProcess
p=pathlib.Path(sys.argv[2])
child=IsolatedProcess(p,expected_sha256=hashlib.sha256(p.read_bytes()).hexdigest())
print(json.dumps({'pid':child.pid,'directory':str(child.directory),'profile':child.profile}),flush=True)
time.sleep(30)
'''
        owner = subprocess.Popen([sys.executable, '-c', owner_code, str(pathlib.Path(__file__).parent), str(probe)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
        owned = None
        try:
            owned = json.loads(owner.stdout.readline())
            assert alive(owned['pid'])
            owner.kill(); owner.communicate(timeout=5)
            deadline = time.monotonic() + 5
            while alive(owned['pid']) and time.monotonic() < deadline:
                time.sleep(0.03)
            assert not alive(owned['pid'])
        finally:
            if owner.poll() is None: owner.kill(); owner.communicate(timeout=5)
            if owned is not None:
                # Remove ONLY this test's known one-file scratch and uniquely generated profile.
                directory = pathlib.Path(owned['directory'])
                assert directory.parent.resolve() == pathlib.Path(tempfile.gettempdir()).resolve() and directory.name.startswith('opennovel-native-')
                assert owned['profile'].startswith('OpenNovel.ReadOnly.')
                deadline = time.monotonic() + 3
                while True:
                    try: (directory / 'engine.exe').unlink(); directory.rmdir(); break
                    except PermissionError:
                        if time.monotonic() >= deadline: raise
                        time.sleep(0.05)
                delete = ctypes.WinDLL('userenv').DeleteAppContainerProfile
                delete.argtypes, delete.restype = [wintypes.LPCWSTR], ctypes.c_long
                assert delete(owned['profile']) >= 0
        checks['forced_parent_exit_kills_engine'] = True

        # Start the exact bundled worker with a deliberately empty PATH.
        env = {k: v for k, v in os.environ.items() if not k.startswith(('PYTHON', 'OPENNOVEL_'))}
        env.update(PATH='', OPENNOVEL_ENGINE_ROOT=str(component))
        worker = subprocess.Popen([str(component / 'python/python.exe'), '-B', '-u', str(component / 'runtime/desktop_engine_worker.py'), '--fixture'], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            def send(value): worker.stdin.write((json.dumps(value) + '\n').encode()); worker.stdin.flush()
            send({'op': 'configure', 'book_id': '10000000001', 'items': ['10000000002']})
            assert json.loads(worker.stdout.readline())['op'] == 'ready'
            send({'op': 'read', 'item_id': '10000000002', 'sequence': 1})
            assert json.loads(worker.stdout.readline())['op'] == 'chapter'
            send({'op': 'cancel'})
            worker.communicate(timeout=10)
            assert worker.returncode == 0
        finally:
            if worker.poll() is None: worker.kill(); worker.communicate(timeout=5)
        checks['bundled_worker_without_path_development_tools'] = True
        report['passed'] = True
    finally:
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--component', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    run(args.component.resolve(), args.output.resolve())
