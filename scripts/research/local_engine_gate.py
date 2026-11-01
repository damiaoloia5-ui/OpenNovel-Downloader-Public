"""Offline evaluation broker. No forwarding, official requests or production entry point."""
import hashlib
import hmac
import json
import os
import re
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qsl, urlsplit

PATH = '/api/novel/book/reader/content/v1'
ITEM = '10000000002'
TEXT = '<p>山间的小屋亮起一盏灯，旅人把湿透的外衣挂在门旁。</p>'
PARAMS = {
    'aid': '13', 'app_name': 'novelapp', 'channel': '0',
    'device_platform': 'android', 'device_type': '25053RT47C',
    'item_id': ITEM, 'os_version': '10', 'support_image': '1', 'version_code': '66.9',
}


def decision(method, target, headers, token):
    """Accept one exact fixture contract, not arbitrary URLs or general GETs."""
    if method != 'GET' or len(target) > 2048:
        return 403, 'denied_method_or_size'
    # The isolated engine adds a per-run capability to the mock origin prefix.
    prefix = '/' + token
    if not hmac.compare_digest(target[:len(prefix)], prefix):
        return 401, 'denied_capability'
    try:
        parsed = urlsplit(target[len(prefix):])
    except ValueError:
        return 403, 'denied_route'
    if parsed.scheme or parsed.netloc or parsed.fragment or parsed.path != PATH:
        return 403, 'denied_route'
    try:
        pairs = parse_qsl(parsed.query, keep_blank_values=True, strict_parsing=True, max_num_fields=16)
    except ValueError:
        return 403, 'denied_query'
    if len(pairs) != len(PARAMS) or dict(pairs) != PARAMS:
        return 403, 'denied_query'
    lowered = {}
    for key, value in headers:
        key = key.lower()
        if key in lowered:
            return 403, 'denied_duplicate_header'
        lowered[key] = value
    if any(key in lowered for key in ('cookie', 'authorization', 'proxy-authorization', 'transfer-encoding')):
        return 403, 'denied_credentials_or_body'
    if lowered.get('content-length', '0') != '0':
        return 403, 'denied_credentials_or_body'
    return 200, 'fixture_allowed'


class Handler(BaseHTTPRequestHandler):
    server_version = 'OpenNovelOfflineGate/1'

    def log_message(self, *_args):
        pass  # Never log request targets, capability, signatures or body.

    def handle_one_request(self):
        self.connection.settimeout(3)
        return super().handle_one_request()

    def respond(self):
        status, outcome = decision(self.command, self.path, self.headers.raw_items(), self.server.token)
        print(json.dumps({'status': status, 'outcome': outcome}), flush=True)
        if status == 200:
            body = {'code': 0, 'data': {'item_id': ITEM, 'content': TEXT}}
        else:
            body = {'code': -1, 'error': outcome}
        data = json.dumps(body, ensure_ascii=False).encode('utf-8')
        self.send_response(status)
        self.send_header('Content-Type', 'application/json; charset=utf-8')
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(data)
        self.close_connection = True

    do_GET = respond
    do_POST = respond
    do_PUT = respond
    do_DELETE = respond
    do_PATCH = respond
    do_CONNECT = respond
    do_HEAD = respond
    do_OPTIONS = respond


if __name__ == '__main__':
    capability = os.environ.get('OPENNOVEL_REVIEW_CAPABILITY', '')
    if not re.fullmatch('[0-9a-f]{64}', capability):
        raise SystemExit('per-run evaluation capability required')
    server = HTTPServer(('127.0.0.1', 8123), Handler)
    server.token = capability
    print(json.dumps({'ready': True, 'fixture_sha256': hashlib.sha256(TEXT.encode()).hexdigest()}), flush=True)
    server.serve_forever()
