"""Bounded public-chapter transport; verified TLS, no credentials or redirects."""
import base64
import http.client
import ipaddress
import math
import socket
import ssl
import threading
import time
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
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
    if status == 429 or (status == 503 and 'retry-after' in headers):
        return 'rate_limited'
    if 300 <= status < 400:
        return 'redirect_refused'
    if not 200 <= status < 300:
        return 'http_error'
    if len(body) > LIMIT:
        return 'oversized'
    return 'empty_response' if not body else 'response_received_unverified'


def retry_after_seconds(value):
    try:
        seconds = int(value)
    except (ValueError, TypeError):
        try:
            date = parsedate_to_datetime(value)
            if date.tzinfo is None:
                date = date.replace(tzinfo=timezone.utc)
            seconds = math.ceil((date - datetime.now(timezone.utc)).total_seconds())
        except (ValueError, TypeError, OverflowError):
            seconds = 60
    return min(86400, max(1, seconds))


class ChapterTransport:
    """One sequential keep-alive connection, owned and closed by one engine session."""
    def __init__(self):
        self._connection = None
        self._context = None
        self._closed = False
        self._lock = threading.Lock()

    def connection(self, timeout):
        with self._lock:
            if self._closed:
                raise OSError('transport closed')
            if self._connection is None:
                if self._context is None:
                    self._context = ssl.create_default_context()
                self._connection = PinnedConnection(HOST, timeout=timeout, context=self._context)
            self._connection.timeout = timeout
            if self._connection.sock is not None:
                self._connection.sock.settimeout(timeout)
            return self._connection

    def discard(self, connection):
        with self._lock:
            if self._connection is connection:
                self._connection = None
        connection.close()

    def close(self):
        with self._lock:
            self._closed = True
            connection, self._connection = self._connection, None
        if connection is not None:
            connection.close()


def fetch_once(request, expected_item, *, timeout=12, transport=None):
    target, headers = validate_request(request, expected_item)
    connection = None
    reusable = False
    try:
        connection = transport.connection(min(12, timeout)) if transport is not None else PinnedConnection(HOST, timeout=min(12, timeout), context=ssl.create_default_context())
        connection.request('GET', target, headers=headers)
        response = connection.getresponse()
        relevant = {k.lower(): v for k, v in response.getheaders()
                    if k.lower() in {'y', 'c', 'content-encoding', 'content-type', 'bdturing-verify', 'retry-after'}}
        # Never read challenges, redirects or failed HTTP bodies.
        body = bytearray()
        if 200 <= response.status < 300 and 'bdturing-verify' not in relevant:
            deadline = time.monotonic() + min(12, timeout)
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
        if outcome == 'rate_limited':
            report['retry_after_seconds'] = retry_after_seconds(relevant.get('retry-after'))
        if outcome != 'response_received_unverified':
            status = response.status if outcome in {'http_error', 'rate_limited'} else {
                'access_challenge': 403, 'oversized': 413}.get(outcome, 502)
            return {'status': status, 'headers': [], 'body': ''}, report
        if any(len(v) > LIMIT for v in relevant.values()):
            raise ValueError('response headers too large')
        forwarded = [[k, v] for k, v in relevant.items() if k not in {'bdturing-verify', 'retry-after'}]
        reusable = transport is not None and not response.will_close
        return {'status': response.status, 'headers': forwarded,
                'body': base64.b64encode(body).decode('ascii')}, report
    except (OSError, http.client.HTTPException, ValueError):
        return {'status': 502, 'headers': [], 'body': ''}, {'outcome': 'transport_failed'}
    finally:
        if connection is not None and not reusable:
            if transport is not None:
                transport.discard(connection)
            else:
                connection.close()


def fetch_with_retries(request, expected_item, *, stop_requested=lambda: False, transport=None):
    """At most two ordinary GETs; never retry challenges, redirects or rate limits."""
    deadline = time.monotonic() + 18
    for attempt in (1, 2):
        if stop_requested() or time.monotonic() >= deadline:
            return {'status': 503, 'headers': [], 'body': ''}, {'outcome': 'cancelled', 'attempts': attempt - 1}
        result, report = fetch_once(request, expected_item, timeout=max(0.01, deadline - time.monotonic()), transport=transport)
        report['attempts'] = attempt
        transient = report['outcome'] in {'transport_failed', 'empty_response'} or (
            report['outcome'] == 'http_error' and report.get('http_status') in (408, 502, 503, 504))
        if not transient or attempt == 2:
            return result, report
        until = min(deadline, time.monotonic() + 1)
        while time.monotonic() < until:
            if stop_requested():
                return {'status': 503, 'headers': [], 'body': ''}, {'outcome': 'cancelled', 'attempts': attempt}
            time.sleep(min(0.03, max(0, until - time.monotonic())))
    raise AssertionError('bounded retry sequence')
