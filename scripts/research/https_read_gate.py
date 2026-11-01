"""One approved public-chapter transport; no credentials, redirects or retry."""
import base64
import http.client
import ipaddress
import socket
import ssl
import time
from urllib.parse import parse_qsl, urlsplit

from local_engine_gate import PARAMS, PATH

HOST = 'novel.snssdk.com'
LIMIT = 2 * 1024 * 1024
HEADERS = {'user-agent', 'accept-encoding', 'y', 'x-argus', 'x-ladon',
           'x-khronos', 'x-ss-req-ticket', 'accept', 'host'}


def validate_request(request, expected_item):
    if set(request) != {'method', 'target', 'headers'} or request['method'] != 'GET':
        raise ValueError('request refused')
    parsed = urlsplit(request['target'])
    if parsed.scheme or parsed.netloc or parsed.fragment or parsed.path != PATH:
        raise ValueError('route refused')
    pairs = parse_qsl(parsed.query, strict_parsing=True, keep_blank_values=True, max_num_fields=16)
    expected = dict(PARAMS, item_id=expected_item)
    if len(pairs) != len(expected) or dict(pairs) != expected:
        raise ValueError('parameters refused')
    headers = {}
    if len(request['headers']) > 16:
        raise ValueError('headers refused')
    for key, value in request['headers']:
        key = key.lower()
        if key not in HEADERS or key in headers or len(value) > 4096 or not value.isascii():
            raise ValueError('headers refused')
        if any(ord(c) < 32 or ord(c) == 127 for c in value):
            raise ValueError('header control character')
        headers[key] = value
    # Host is generated locally; the container's private loopback authority is never forwarded.
    headers.pop('host', None)
    return parsed.path + '?' + parsed.query, headers


class PinnedConnection(http.client.HTTPSConnection):
    def connect(self):
        addresses = socket.getaddrinfo(HOST, 443, type=socket.SOCK_STREAM)
        if not addresses or any(not ipaddress.ip_address(item[4][0]).is_global for item in addresses):
            raise OSError('public endpoint required')
        family, kind, protocol, _, address = addresses[0]
        sock = socket.socket(family, kind, protocol)
        try:
            sock.settimeout(self.timeout)
            sock.connect(address)
            self.sock = self._context.wrap_socket(sock, server_hostname=HOST)
        except BaseException:
            sock.close()
            raise


def classify(status, headers, body):
    if 'bdturing-verify' in headers or status in (401, 403):
        return 'access_challenge'
    if status == 429:
        return 'rate_limited'
    if 300 <= status < 400:
        return 'redirect_refused'
    if not 200 <= status < 300:
        return 'http_error'
    if len(body) > LIMIT:
        return 'oversized'
    return 'empty_response' if not body else 'response_received_unverified'


def fetch_once(request, expected_item):
    target, headers = validate_request(request, expected_item)
    connection = PinnedConnection(HOST, timeout=12, context=ssl.create_default_context())
    try:
        connection.request('GET', target, headers=headers)
        response = connection.getresponse()
        relevant = {k.lower(): v for k, v in response.getheaders()
                    if k.lower() in {'y', 'c', 'content-encoding', 'content-type', 'bdturing-verify'}}
        # Never read challenges, redirects or failed HTTP bodies.
        body = bytearray()
        if 200 <= response.status < 300 and 'bdturing-verify' not in relevant:
            deadline = time.monotonic() + 12
            while len(body) <= LIMIT:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('response budget exceeded')
                if connection.sock is not None:
                    connection.sock.settimeout(min(12, remaining))
                chunk = response.read1(min(65536, LIMIT + 1 - len(body)))
                if not chunk:
                    break
                body.extend(chunk)
        outcome = classify(response.status, relevant, body)
        report = {'http_status': response.status, 'read_bytes': len(body), 'outcome': outcome}
        if outcome != 'response_received_unverified':
            return {'status': 403, 'headers': [], 'body': ''}, report
        if any(len(v) > LIMIT for v in relevant.values()):
            raise ValueError('response headers too large')
        forwarded = [[k, v] for k, v in relevant.items() if k != 'bdturing-verify']
        return {'status': response.status, 'headers': forwarded,
                'body': base64.b64encode(body).decode('ascii')}, report
    except (OSError, http.client.HTTPException, ValueError):
        return {'status': 502, 'headers': [], 'body': ''}, {'outcome': 'transport_failed'}
    finally:
        connection.close()
