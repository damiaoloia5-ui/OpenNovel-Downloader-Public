"""Actual HTTP refusal checks, executed inside the offline broker namespace."""
import hashlib
import http.client
import json
import os
import socket
from urllib.parse import urlencode

from local_engine_gate import PARAMS, PATH, TEXT

token = os.environ['OPENNOVEL_REVIEW_CAPABILITY']
target = '/' + token + PATH + '?' + urlencode(PARAMS)
cases = [
    ('GET', target, {}, 200),
    ('GET', PATH + '?' + urlencode(PARAMS), {}, 401),
    ('GET', target.replace(token, '0' * 64), {}, 401),
    ('POST', target, {}, 403),
    ('CONNECT', 'example.invalid:443', {}, 403),
    ('GET', '/' + token + '/service/2/device_register/', {}, 403),
    ('GET', '/' + token + '/reading/user/privilege/add/v/', {}, 403),
    ('GET', '/' + token + '/reading/crypt/registerkey', {}, 403),
    ('GET', '/' + token + '/api/v1/account/fanqie/library', {}, 403),
    ('GET', '/' + token + '/api/v1/comment/add', {}, 403),
    ('GET', '/' + token + '/src/private', {}, 403),
    ('GET', '/' + token + '/reading/reader/full/v/', {}, 403),
    ('GET', target + '&item_id=' + PARAMS['item_id'], {}, 403),
    ('GET', target, {'Cookie': 'original-test-value'}, 403),
    ('GET', target, {'Authorization': 'original-test-value'}, 403),
]
for method, path, headers, expected in cases:
    client = http.client.HTTPConnection('127.0.0.1', 8123, timeout=3)
    client.request(method, path, headers=headers)
    response = client.getresponse()
    body = response.read(4096)
    assert response.status == expected, (method, expected, response.status)
    if expected == 200:
        assert json.loads(body)['data']['content'] == TEXT
    assert token.encode() not in body
    client.close()
try:
    socket.create_connection(('192.0.2.1', 443), timeout=0.5).close()
except OSError:
    pass
else:
    raise AssertionError('unexpected external connectivity')
print(json.dumps({'http_cases_passed': len(cases), 'external_probe_blocked': True,
                  'fixture_sha256': hashlib.sha256(TEXT.encode()).hexdigest()}))
