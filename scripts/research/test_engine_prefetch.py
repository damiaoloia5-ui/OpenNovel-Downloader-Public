"""Original concurrency fixtures, no processes, accounts or external requests."""
import threading
import time
import unittest

from desktop_engine_worker import EngineStopped, PrefetchedSession


class PrefetchTests(unittest.TestCase):
    def make(self, fail=None):
        owner = self
        self.calls, self.sessions = [], []
        self.active, self.peak = 0, 0
        self.lock = threading.Lock()
        self.release = threading.Event()
        self.started_three = threading.Event()
        self.items = [str(10000000002+i) for i in range(8)]
        self.stop = threading.Event()
        class Session:
            def __init__(self, *_args, **_kwargs):
                self.closed = False
                owner.sessions.append(self)
            def request(self, item, *, cancelled):
                with owner.lock:
                    owner.calls.append(item)
                    owner.active += 1
                    owner.peak = max(owner.peak, owner.active)
                    if owner.active == 3: owner.started_three.set()
                try:
                    if item == fail:
                        raise EngineStopped('controlled limit', 'rate_limited', {'retry_after_seconds': 7})
                    while not owner.release.wait(.005):
                        if cancelled.is_set(): raise EngineStopped('cancelled')
                    if cancelled.is_set(): raise EngineStopped('cancelled')
                    return {'item_id': item}
                finally:
                    with owner.lock: owner.active -= 1
            def close(self): self.closed = True
        return PrefetchedSession('10000000001', self.items, factory=Session, cancelled=self.stop)

    def test_three_requests_overlap_but_results_remain_ordered_and_bounded(self):
        reader = self.make()
        result = []
        thread = threading.Thread(target=lambda: result.append(reader.request(self.items[0], cancelled=self.stop)))
        thread.start()
        try:
            self.assertTrue(self.started_three.wait(2))
            self.assertEqual(set(self.calls), set(self.items[:3]))
            self.release.set();thread.join(2)
            self.assertEqual(result[0]['item_id'], self.items[0])
            for item in self.items[1:]:
                self.assertEqual(reader.request(item, cancelled=self.stop)['item_id'], item)
                self.assertLessEqual(len(reader.pending), 2)
            self.assertEqual(len(self.calls), len(self.items))
            self.assertLessEqual(self.peak, 3)
        finally:
            reader.close();thread.join(2)
        self.assertTrue(all(s.closed for s in self.sessions))

    def test_failure_stops_window_without_more_requests_or_dropping_reason(self):
        reader = self.make(fail='10000000003')
        try:
            with self.assertRaises(EngineStopped) as raised:
                reader.request(self.items[0], cancelled=self.stop)
            self.assertEqual(raised.exception.kind, 'rate_limited')
            count = len(self.calls)
            with self.assertRaises(EngineStopped): reader.request(self.items[4], cancelled=self.stop)
            self.assertEqual(len(self.calls), count)
            self.assertLessEqual(count, 3)
        finally: reader.close()

    def test_cancel_never_returns_a_late_body_and_unknown_item_never_requests(self):
        reader = self.make();errors=[]
        try:
            with self.assertRaises(EngineStopped): reader.request('99999999999', cancelled=self.stop)
            self.assertEqual(self.calls, [])
            def consume():
                try: reader.request(self.items[0], cancelled=self.stop)
                except EngineStopped: errors.append(True)
            thread=threading.Thread(target=consume);thread.start()
            self.assertTrue(self.started_three.wait(2));self.stop.set();thread.join(2)
            self.assertEqual(errors,[True])
        finally:reader.close()

    def test_skipped_cached_items_do_not_grow_the_window_or_mix_results(self):
        reader=self.make();self.release.set()
        try:
            for item in [self.items[0],self.items[5],self.items[6],self.items[7]]:
                self.assertEqual(reader.request(item,cancelled=self.stop)['item_id'],item)
                self.assertLessEqual(len(reader.pending),2)
            self.assertNotIn(self.items[3],self.calls)
            self.assertNotIn(self.items[4],self.calls)
        finally:reader.close()

    def test_close_waits_for_active_requests_before_closing_native_handles(self):
        reader = self.make()
        closed_while_active = []
        for session in self.sessions:
            original_close = session.close
            def close(original_close=original_close):
                with self.lock:
                    if self.active:
                        closed_while_active.append(True)
                original_close()
            session.close = close
        # Exercise shutdown with all workers blocked in a request. The caller
        # has already stopped consuming, as happens when the stdio job exits.
        for item in self.items[:3]:
            reader.pending[item] = reader.executor.submit(reader._fetch, item)
        try:
            self.assertTrue(self.started_three.wait(2))
        finally:
            reader.close()
        self.assertEqual(closed_while_active, [])
        self.assertEqual(self.active, 0)
        self.assertTrue(all(session.closed for session in self.sessions))
