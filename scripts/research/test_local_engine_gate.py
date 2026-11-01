import unittest
from urllib.parse import urlencode

from local_engine_gate import PARAMS, PATH, decision

TOKEN = 'a' * 64
TARGET = '/' + TOKEN + PATH + '?' + urlencode(PARAMS)


class GatePolicyTests(unittest.TestCase):
    def test_only_exact_fixture_is_allowed(self):
        self.assertEqual(decision('GET', TARGET, [], TOKEN), (200, 'fixture_allowed'))

    def test_write_verbs_tunnels_and_missing_capability_are_denied(self):
        for method in ['POST', 'PUT', 'DELETE', 'PATCH', 'CONNECT', 'OPTIONS', 'HEAD']:
            self.assertNotEqual(decision(method, TARGET, [], TOKEN)[0], 200)
        for target in [PATH, '/' + 'b' * 64 + PATH, 'https://example.com' + PATH]:
            self.assertNotEqual(decision('GET', target, [], TOKEN)[0], 200)

    def test_register_privilege_account_comments_media_and_traversal_are_denied(self):
        for path in ['/service/2/device_register/', '/reading/user/privilege/add/v/',
                     '/reading/crypt/registerkey', '/api/v1/account/fanqie/library',
                     '/api/v1/comment/add', '/src/secret', '/reading/reader/full/v/',
                     '/reading/reader/batch_full/v', '//example.com/x', '/a/../' + PATH,
                     'http://[invalid',
                     '/%61pi/novel/book/reader/content/v1']:
            self.assertNotEqual(decision('GET', '/' + TOKEN + path, [], TOKEN)[0], 200)

    def test_extra_duplicate_malformed_and_wrong_identity_parameters_are_denied(self):
        for suffix in ['&item_id=' + PARAMS['item_id'], '&url=https://example.com',
                       '&cookie=secret', '&broken', '#fragment']:
            self.assertNotEqual(decision('GET', TARGET + suffix, [], TOKEN)[0], 200)
        self.assertNotEqual(decision('GET', TARGET.replace(PARAMS['item_id'], '10000000009'), [], TOKEN)[0], 200)

    def test_credentials_bodies_and_duplicate_headers_are_denied(self):
        for headers in [[('Cookie', 'synthetic')], [('Authorization', 'synthetic')],
                        [('Proxy-Authorization', 'synthetic')], [('Transfer-Encoding', 'chunked')],
                        [('Content-Length', '1')], [('Host', 'a'), ('host', 'b')]]:
            self.assertNotEqual(decision('GET', TARGET, headers, TOKEN)[0], 200)


if __name__ == '__main__':
    unittest.main()
