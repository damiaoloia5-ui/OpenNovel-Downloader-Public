import unittest
import json
import ssl
from unittest.mock import patch
from urllib.parse import urlencode

from https_read_gate import LIMIT, PinnedConnection, classify, fetch_once, validate_request
from local_engine_gate import ITEM, PARAMS, PATH
from run_https_read_gate import METADATA_FIELDS, split_engine_output


class HttpsReadGateTests(unittest.TestCase):
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
