"""Explicit Docker-only controlled session checks. Never requests a real platform."""
import json
import pathlib
import threading
import time

from persistent_engine import EngineSession, EngineStopped, original_fixture
from run_local_engine_gate import REPO

BOOK, ITEMS = '10000000001', ['10000000002', '10000000003']


def main():
    reports = []
    def record(name, session):
        identifiers = [item['container'] for item in session.evidence]
        for identifier in identifiers:
            try:
                session._docker('inspect', identifier)
            except EngineStopped:
                pass
            else:
                raise AssertionError('owned container survived cleanup')
        reports.append({'case': name, 'requests': session.requests, 'containers_created': len(identifiers), 'cleanup_verified': True})

    started = time.monotonic()
    with EngineSession(BOOK, ITEMS, fixture=original_fixture) as session:
        identifier = session.engine
        for item in [ITEMS[0], ITEMS[1], ITEMS[0]]:
            envelope = session.request(item)
            assert envelope['item_id'] == item and envelope['need_pay'] == 0
            assert session.engine == identifier
        assert session.requests == 3
        try:
            session.request('10000000009')
        except ValueError:
            pass
        else:
            raise AssertionError('unknown chapter accepted')
        assert session.requests == 3
    record('reuse_and_unknown_identity', session)

    with EngineSession(BOOK, ITEMS, fixture=lambda _item: {'status': 403, 'headers': [], 'body': ''}) as challenged:
        try:
            challenged.request(ITEMS[0])
        except EngineStopped:
            pass
        else:
            raise AssertionError('challenge accepted')
        try:
            challenged.request(ITEMS[1])
        except EngineStopped:
            pass
        else:
            raise AssertionError('closed challenged session continued')
        assert challenged.requests == 1
    record('challenge_stops_session', challenged)

    with EngineSession(BOOK, ITEMS, fixture=original_fixture) as restarted:
        assert restarted.engine != challenged.engine
        assert restarted.request(ITEMS[1])['item_id'] == ITEMS[1]
    record('fresh_session_after_challenge', restarted)

    entered, release, cancelled = threading.Event(), threading.Event(), threading.Event()
    def slow(item):
        entered.set()
        release.wait(8)
        return original_fixture(item)
    outcome = []
    with EngineSession(BOOK, ITEMS, fixture=slow) as stopping:
        def request():
            try:
                stopping.request(ITEMS[0], cancelled=cancelled)
                outcome.append('unexpected_body')
            except EngineStopped:
                outcome.append('cancelled')
        worker = threading.Thread(target=request)
        worker.start()
        assert entered.wait(15)
        cancelled.set()
        worker.join(12)
        release.set()
        assert not worker.is_alive() and outcome == ['cancelled']
    record('cancel_discards_inflight_result', stopping)

    with EngineSession(BOOK, ITEMS, fixture=original_fixture) as stale:
        stale._responses.put_nowait({'sequence': 999, 'ok': True, 'envelope': {}})
        try:
            stale.request(ITEMS[0])
        except EngineStopped:
            pass
        else:
            raise AssertionError('stale response accepted')
    record('stale_sequence_refused', stale)

    with EngineSession(BOOK, ITEMS, fixture=original_fixture) as exited:
        exited._docker('stop', '--time', '1', exited.engine)
        exited._child.wait(timeout=5)
        try:
            exited.request(ITEMS[0])
        except EngineStopped:
            pass
        else:
            raise AssertionError('exited process was reused')
        assert exited.requests == 0
    record('dead_process_not_reused', exited)

    result = {'controlled_only': True, 'checks_passed': len(reports), 'elapsed_seconds': round(time.monotonic()-started, 3), 'cases': reports}
    output = REPO / '.artifacts/persistent-engine-20261101'
    output.mkdir(parents=True, exist_ok=True)
    (output / 'controlled-session-checks.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
