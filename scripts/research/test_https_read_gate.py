import unittest
import json
import ssl
from unittest.mock import patch
from urllib.parse import urlencode

from https_read_gate import ChapterTransport, LIMIT, PinnedConnection, RequestPacer, classify, fetch_once, fetch_with_retries, validate_request
from local_engine_gate import ITEM, PARAMS, PATH
from run_https_read_gate import METADATA_FIELDS, split_engine_output


class HttpsReadGateTests(unittest.TestCase):
    def setUp(self):
        self.pacer_patch = patch('https_read_gate._PACER', RequestPacer())
        self.pacer_patch.start()
        self.addCleanup(self.pacer_patch.stop)

    def test_pacing_is_shared_and_cancelable_without_a_real_request(self):
        clock = [0.0]
        def sleep(seconds): clock[0] += seconds
        pacer = RequestPacer(clock=lambda: clock[0], sleep=sleep)
        starts=[]
        for _ in range(4):
            self.assertIsNone(pacer.acquire(lambda: False)); starts.append(clock[0])
        self.assertEqual(starts, [0, .5, 1, 1.5])
        self.assertEqual(pacer.acquire(lambda: True)['outcome'], 'cancelled')
        pacer.observe({'outcome':'rate_limited','retry_after_seconds':120})
        pacer.observe({'outcome':'rate_limited','retry_after_seconds':1})
        self.assertEqual(pacer.acquire(lambda: False)['retry_after_seconds'],120)
        clock[0] += 120
        self.assertIsNone(pacer.acquire(lambda: False))

    def test_new_call_during_cooldown_never_reaches_transport(self):
        with patch('https_read_gate.fetch_once', return_value=({'status':429}, {'outcome':'rate_limited','retry_after_seconds':60})) as fetch:
            fetch_with_retries(self.request(), ITEM)
            result,report=fetch_with_retries(self.request(), ITEM)
            self.assertEqual(fetch.call_count,1)
            self.assertEqual(result['status'],429)
            self.assertEqual(report['attempts'],0)

    def test_keep_alive_reuses_only_successful_fully_read_responses(self):
        class Response:
            will_close = False
            def __init__(self, status=200):
                self.status, self.reads = status, 0
            def getheaders(self): return []
            def read1(self, _limit):
                self.reads += 1
                return b'original fixture' if self.reads == 1 else b''
        class Connection:
            sock = None
            def __init__(self, *_a, **_kw): self.closed = 0
            def request(self, *_a, **_kw): pass
            def getresponse(self): return responses.pop(0)
            def close(self): self.closed += 1
        responses = [Response(), Response(), Response(403), Response()]
        with patch('https_read_gate.PinnedConnection', side_effect=Connection) as make:
            transport = ChapterTransport()
            for expected in [200, 200, 403, 200]:
                result, _ = fetch_once(self.request(), ITEM, transport=transport)
                self.assertEqual(result['status'], expected)
            self.assertEqual(make.call_count, 2)
            transport.close()
            result, report = fetch_once(self.request(), ITEM, transport=transport)
            self.assertEqual(report['outcome'], 'transport_failed')
            self.assertEqual(make.call_count, 2)

    def test_keep_alive_does_not_reuse_broken_oversized_or_close_responses(self):
        class Response:
            status = 200
            def __init__(self, large=False, closing=False):
                self.large, self.will_close, self.reads = large, closing, 0
            def getheaders(self): return []
            def read1(self, limit):
                self.reads += 1
                return b'x' * limit if self.large else b'ok' if self.reads == 1 else b''
        class Connection:
            sock = None
            def __init__(self, *_a, **_kw): pass
            def request(self, *_a, **_kw): pass
            def getresponse(self):
                value = responses.pop(0)
                if isinstance(value, Exception): raise value
                return value
            def close(self): pass
        responses = [Response(large=True), OSError('closed socket'), Response(closing=True), Response()]
        with patch('https_read_gate.PinnedConnection', side_effect=Connection) as make:
            transport = ChapterTransport()
            for outcome in ['oversized', 'transport_failed', 'response_received_unverified', 'response_received_unverified']:
                _, report = fetch_once(self.request(), ITEM, transport=transport)
                self.assertEqual(report['outcome'], outcome)
            self.assertEqual(make.call_count, 4)
            transport.close()

    def test_transient_empty_response_retries_once_and_then_recovers(self):
        failure = ({'status': 502, 'headers': [], 'body': ''}, {'outcome': 'empty_response', 'http_status': 200})
        success = ({'status': 200, 'headers': [], 'body': 'original-fixture'}, {'outcome': 'response_received_unverified', 'http_status': 200})
        with patch('https_read_gate.fetch_once', side_effect=[failure, success]) as fetch:
            result, report = fetch_with_retries(self.request(), ITEM)
        self.assertEqual(fetch.call_count, 2)
        self.assertEqual(result['status'], 200)
        self.assertEqual(report['attempts'], 2)

    def test_persistent_transient_failure_has_only_two_attempts(self):
        with patch('https_read_gate.fetch_once', side_effect=lambda *_a, **_kw: ({'status': 502, 'headers': [], 'body': ''}, {'outcome': 'transport_failed'})) as fetch:
            result, report = fetch_with_retries(self.request(), ITEM)
        self.assertEqual(fetch.call_count, 2)
        self.assertEqual(result['status'], 502)
        self.assertEqual(report['attempts'], 2)

    def test_challenges_limits_redirects_and_oversize_are_never_retried(self):
        for outcome in ['access_challenge', 'rate_limited', 'redirect_refused', 'oversized']:
            with patch('https_read_gate._PACER', RequestPacer()), patch('https_read_gate.fetch_once', return_value=({'status': 403, 'headers': [], 'body': ''}, {'outcome': outcome})) as fetch:
                fetch_with_retries(self.request(), ITEM)
            self.assertEqual(fetch.call_count, 1)

    def test_cancelled_retry_does_not_send_another_request(self):
        stopped = False
        def fetch(*_args, **_kwargs):
            nonlocal stopped
            stopped = True
            return {'status': 502, 'headers': [], 'body': ''}, {'outcome': 'empty_response'}
        with patch('https_read_gate.fetch_once', side_effect=fetch) as call:
            result, report = fetch_with_retries(self.request(), ITEM, stop_requested=lambda: stopped)
        self.assertEqual(call.call_count, 1)
        self.assertEqual(report['outcome'], 'cancelled')
        self.assertEqual(result['body'], '')

    def test_empty_rate_limited_and_redirected_responses_keep_distinct_causes(self):
        class Response:
            def __init__(self, status, headers):
                self.status, self.headers = status, headers
            def getheaders(self):
                return self.headers
            def read1(self, _limit):
                return b''
        class Connection:
            sock = None
            def request(self, *_args, **_kwargs):
                pass
            def getresponse(self):
                return response
            def close(self):
                pass
        for status, headers, forwarded_status, outcome in [
            (429, [('Retry-After', '7')], 429, 'rate_limited'),
            (503, [('Retry-After', '11')], 503, 'rate_limited'),
            (200, [], 502, 'empty_response'),
            (302, [], 502, 'redirect_refused'),
            (403, [], 403, 'access_challenge'),
        ]:
            response = Response(status, headers)
            with patch('https_read_gate.PinnedConnection', return_value=Connection()):
                forwarded, report = fetch_once(self.request(), ITEM)
            self.assertEqual(forwarded['status'], forwarded_status)
            self.assertEqual(report['outcome'], outcome)
            self.assertEqual(forwarded['body'], '')
            if outcome == 'rate_limited':
                self.assertEqual(report['retry_after_seconds'], int(headers[0][1]))

    def test_real_transport_bounds_error_bodies_redirects_and_oversized_responses(self):
        class FakeConnection:
            def __init__(self, *_args, **_kwargs):
                self.calls = 0
                self.sock = None
            def request(self, *_args, **_kwargs):
                self.calls += 1
            def getresponse(self):
                return response
            def close(self):
                pass
        class Response:
            def __init__(self, status):
                self.status, self.reads = status, []
            def getheaders(self):
                return []
            def read1(self, limit):
                self.reads.append(limit)
                return b'x' * limit
        for status in [302, 403, 429, 500, 200]:
            response = Response(status)
            connection = FakeConnection()
            with patch('https_read_gate.PinnedConnection', return_value=connection):
                forwarded, report = fetch_once(self.request(), ITEM)
            self.assertEqual(connection.calls, 1)
            self.assertEqual(forwarded['body'], '')
            self.assertEqual(sum(response.reads), LIMIT + 1 if status == 200 else 0)
            self.assertNotEqual(report['outcome'], 'response_received_unverified')

    def test_private_dns_answer_is_refused_and_tls_verification_is_enabled(self):
        context = ssl.create_default_context()
        self.assertTrue(context.check_hostname)
        self.assertEqual(context.verify_mode, ssl.CERT_REQUIRED)
        connection = PinnedConnection('novel.snssdk.com', context=context)
        with patch('https_read_gate.socket.getaddrinfo', return_value=[(2, 1, 6, '', ('127.0.0.1', 443))]):
            with self.assertRaises(OSError):
                connection.connect()
    def test_plaintext_envelope_is_removed_before_metadata_reporting(self):
        value = {key: None for key in METADATA_FIELDS}
        value['envelope'] = {'content': 'original-private-fixture'}
        metadata, envelope = split_engine_output(json.dumps(value))
        self.assertEqual(envelope['content'], 'original-private-fixture')
        self.assertNotIn('original-private-fixture', json.dumps(metadata))
        with self.assertRaises(ValueError):
            split_engine_output('{"arbitrary_server_message":"secret"}')
    def request(self, **changes):
        value = {'method': 'GET', 'target': PATH + '?' + urlencode(PARAMS), 'headers': [['Host', '127.0.0.1:8123'], ['y', 'synthetic']]}
        value.update(changes)
        return value

    def test_fixed_contract_and_private_host_removed(self):
        path, headers = validate_request(self.request(), ITEM)
        self.assertTrue(path.startswith(PATH + '?'))
        self.assertNotIn('host', headers)
        self.assertEqual(headers, {'y': 'synthetic'})

    def test_arbitrary_targets_methods_and_identity_never_forward(self):
        for target in ['https://example.invalid/', '//example.invalid/', '/reading/crypt/registerkey',
                       PATH + '?' + urlencode(PARAMS) + '&item_id=' + ITEM]:
            with self.assertRaises(ValueError):
                validate_request(self.request(target=target), ITEM)
        for method in ['POST', 'CONNECT', 'PUT']:
            with self.assertRaises(ValueError):
                validate_request(self.request(method=method), ITEM)
        with self.assertRaises(ValueError):
            validate_request(self.request(), '10000000009')

    def test_sensitive_unknown_duplicate_and_control_headers_rejected(self):
        for headers in [[['Cookie', 'synthetic']], [['Authorization', 'synthetic']],
                        [['Proxy-Authorization', 'synthetic']], [['X-Other', 'x']],
                        [['y', 'x'], ['Y', 'x']], [['y', 'x\r\ny:z']]]:
            with self.assertRaises(ValueError):
                validate_request(self.request(headers=headers), ITEM)

    def test_http_success_is_not_body_acceptance(self):
        for status, headers, body, expected in [
            (200, {}, b'', 'empty_response'), (200, {'bdturing-verify': 'x'}, b'', 'access_challenge'),
            (403, {}, b'', 'access_challenge'), (429, {}, b'', 'rate_limited'),
            (302, {}, b'', 'redirect_refused'), (500, {}, b'', 'http_error'),
            (200, {}, b'x' * (LIMIT + 1), 'oversized'),
            (200, {}, b'original-test', 'response_received_unverified'),
        ]:
            self.assertEqual(classify(status, headers, body), expected)


if __name__ == '__main__':
    unittest.main()
