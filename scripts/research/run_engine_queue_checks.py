"""Controlled persistent engine to real Rust queue, stop/resume and TXT integration."""
import json
import pathlib
import subprocess
import uuid

from persistent_engine import EngineSession, original_fixture
from run_local_engine_gate import REPO

BOOK, ITEMS = '10000000001', ['10000000002', '10000000003', '10000000004']


def main():
    root = REPO / '.artifacts/persistent-engine-20261101' / ('queue-' + uuid.uuid4().hex)
    root.mkdir(parents=True)
    catalog = {'source': 'fanqie-app-local', 'remote_id': BOOK, 'title': '原创会话队列',
               'author': '', 'description': '', 'chapters': [{'id': item, 'title': '原创章', 'restricted': False} for item in ITEMS]}
    path = root / 'catalog.json'
    path.write_text(json.dumps(catalog, ensure_ascii=False), encoding='utf-8')
    executable = REPO / 'target/debug/examples/engine_queue.exe'
    def run(mode, value, stop_after=None):
        seen, calls = [], []
        with EngineSession(BOOK, ITEMS, fixture=original_fixture) as session:
            child = subprocess.Popen([str(executable), str(root / 'library.db'), mode, str(value), str(root / 'exports')],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8')
            try:
                for raw in child.stdout:
                    event = json.loads(raw)
                    if event['op'] != 'read':
                        seen.append(event)
                        continue
                    if stop_after is not None and len(calls) == stop_after:
                        result = {'error': 'cancelled'}
                    else:
                        calls.append(event['chapter_id'])
                        result = session.request(event['chapter_id'])
                    child.stdin.write(json.dumps(result, ensure_ascii=False) + '\n')
                    child.stdin.flush()
                if child.wait(timeout=10):
                    raise AssertionError('queue companion failed')
            finally:
                if child.poll() is None:
                    child.terminate()
                    child.wait(timeout=5)
                for stream in [child.stdin, child.stdout, child.stderr]:
                    stream.close()
        return seen[-1], seen[0]['id'], calls
    partial, job, first_calls = run('start', path, stop_after=1)
    assert partial['job']['saved'] == 1 and partial['job']['state'] == 'cancelled' and partial['export']['partial']
    full, same_job, resumed_calls = run('resume', job)
    assert same_job == job and full['job']['saved'] == 3 and full['job']['state'] == 'completed'
    assert first_calls == ITEMS[:1] and resumed_calls == ITEMS[1:]
    assert full['job']['book_id'] == partial['job']['book_id'] and not full['export']['partial']
    assert pathlib.Path(full['export']['path']).read_text(encoding='utf-8').count('山间的小屋亮起一盏灯。') == 3
    report = {'controlled_only': True, 'same_job_resumed': True, 'saved_before': 1, 'saved_after': 3,
              'resumed_only_missing': resumed_calls == ITEMS[1:], 'partial_txt': True, 'full_fixture_txt': True}
    (root / 'queue-checks.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
