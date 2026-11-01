"""Original in-process desktop protocol checks; no Docker, network or library."""
import json
import os
import queue
import threading
import types
import unittest
from unittest.mock import patch

import desktop_engine_worker as worker


class ProtocolTests(unittest.TestCase):
    def setUp(self):
        read_fd, write_fd = os.pipe()
        self.input = os.fdopen(write_fd, 'wb', buffering=0)
        self.reader = os.fdopen(read_fd, 'rb', buffering=0)
        self.events, self.sessions = queue.Queue(), []
        self.entered = threading.Event()
        self.slow = False
        self.failure_kind = None
        owner = self

        class Session:
            def __init__(self, book, items, **kwargs):
                self.book, self.items, self.options = book, items, kwargs
                self.calls, self.closed = [], False
                owner.sessions.append(self)

            def request(self, item, *, cancelled):
                self.calls.append(item)
                owner.entered.set()
                if owner.failure_kind:
                    raise worker.EngineStopped('private upstream detail never exposed', owner.failure_kind)
                if owner.slow:
                    if not cancelled.wait(3):
                        raise AssertionError('cancel not delivered')
                    raise RuntimeError('cancelled')
                return {'item_id': item, 'book_id': self.book, 'need_pay': 0}

            def close(self):
                self.closed = True

        self.patches = [patch.object(worker, 'sys', types.SimpleNamespace(stdin=types.SimpleNamespace(buffer=self.reader))),
                        patch.object(worker, 'emit', self.events.put), patch.object(worker, 'EngineSession', Session)]
        for value in self.patches:
            value.start()
        self.thread = threading.Thread(target=worker.serve)
        self.thread.start()

    def tearDown(self):
        self.input.close()
        self.thread.join(6)
        self.assertFalse(self.thread.is_alive())
        self.reader.close()
        for value in reversed(self.patches):
            value.stop()

    def send(self, value):
        self.input.write((json.dumps(value) + '\n').encode())

    def configure(self):
        self.send({'op': 'configure', 'book_id': '10000000001', 'items': ['10000000002', '10000000003']})
        self.assertEqual(self.events.get(timeout=3), {'op': 'ready'})

    def test_one_session_serves_sequential_requests_then_cleans_up(self):
        self.configure()
        for sequence, item in enumerate(['10000000002', '10000000003'], 1):
            self.send({'op': 'read', 'item_id': item, 'sequence': sequence})
            value = self.events.get(timeout=3)
            self.assertEqual((value['op'], value['sequence'], value['envelope']['item_id']), ('chapter', sequence, item))
        self.send({'op': 'cancel'})
        self.thread.join(3)
        self.assertEqual(len(self.sessions), 1)
        self.assertTrue(self.sessions[0].closed)
        self.assertTrue(self.sessions[0].options['live'])

    def test_stale_sequence_closes_without_requesting_content(self):
        self.configure()
        self.send({'op': 'read', 'item_id': '10000000002', 'sequence': 42})
        self.assertEqual(self.events.get(timeout=3), {'op': 'error', 'kind': 'stopped'})
        self.thread.join(3)
        self.assertEqual(self.sessions[0].calls, [])
        self.assertTrue(self.sessions[0].closed)

    def test_arbitrary_configuration_is_refused_before_starting_engine(self):
        self.send({'op': 'configure', 'book_id': '10000000001', 'items': [], 'url': 'https://example.invalid/'})
        self.assertEqual(self.events.get(timeout=3), {'op': 'error', 'kind': 'stopped'})
        self.thread.join(3)
        self.assertEqual(self.sessions, [])

    def test_cancel_during_read_never_emits_late_body(self):
        self.slow = True
        self.configure()
        self.send({'op': 'read', 'item_id': '10000000002', 'sequence': 1})
        self.assertTrue(self.entered.wait(3))
        self.send({'op': 'cancel'})
        self.thread.join(3)
        self.assertFalse(self.thread.is_alive())
        self.assertTrue(self.events.empty())
        self.assertTrue(self.sessions[0].closed)

    def test_parent_eof_cancels_active_request(self):
        self.slow = True
        self.configure()
        self.send({'op': 'read', 'item_id': '10000000002', 'sequence': 1})
        self.assertTrue(self.entered.wait(3))
        self.input.close()
        self.thread.join(3)
        self.assertFalse(self.thread.is_alive())
        self.assertTrue(self.events.empty())
        self.assertTrue(self.sessions[0].closed)

    def test_challenge_is_actionable_without_exposing_upstream_details(self):
        self.failure_kind = 'access_challenge'
        self.configure()
        self.send({'op': 'read', 'item_id': '10000000002', 'sequence': 1})
        self.assertEqual(self.events.get(timeout=3), {'op': 'error', 'kind': 'access_challenge'})
        self.thread.join(3)
        self.assertTrue(self.sessions[0].closed)


if __name__ == '__main__':
    unittest.main()
