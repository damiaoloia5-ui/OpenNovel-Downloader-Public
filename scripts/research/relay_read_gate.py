"""Network-free HTTP-to-mailbox relay. Secrets exist only in container tmpfs."""
import base64
import hmac
import json
import os
import pathlib
import re
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

from https_read_gate import validate_request

MAIL = pathlib.Path('/tmp/opennovel-mail')


class Relay(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def reply(self, status, headers=(), body=b''):
        self.send_response(status)
        for key, value in headers:
            if key.lower() in {'y', 'c', 'content-encoding', 'content-type'}:
                self.send_header(key, value)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True

    def do_GET(self):
        prefix = '/' + self.server.capability
        if not hmac.compare_digest(self.path[:len(prefix)], prefix) or self.server.used:
            return self.reply(403)
        request = {'method': 'GET', 'target': self.path[len(prefix):], 'headers': list(self.headers.raw_items())}
        try:
            validate_request(request, self.server.item)
        except (ValueError, TypeError):
            return self.reply(403)
        self.server.used = True
        # The host reads this through Docker exec; it is never emitted to Docker logs.
        temporary = MAIL / 'request.tmp'
        temporary.write_text(json.dumps(request), encoding='utf-8')
        temporary.rename(MAIL / 'request.json')
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if (MAIL / 'response.json').exists():
                result = json.loads((MAIL / 'response.json').read_text(encoding='utf-8'))
                return self.reply(result['status'], result['headers'], base64.b64decode(result['body'], validate=True))
            time.sleep(0.05)
        self.reply(504)

    def refuse(self):
        self.reply(403)

    do_POST = refuse
    do_PUT = refuse
    do_DELETE = refuse
    do_CONNECT = refuse
    do_PATCH = refuse


if __name__ == '__main__':
    capability = os.environ.get('OPENNOVEL_REVIEW_CAPABILITY', '')
    item = os.environ.get('OPENNOVEL_APPROVED_ITEM', '')
    if not re.fullmatch('[0-9a-f]{64}', capability) or not re.fullmatch('[1-9][0-9]{9,23}', item):
        raise SystemExit('approved single request required')
    MAIL.mkdir(mode=0o700)
    server = HTTPServer(('127.0.0.1', 8123), Relay)
    server.capability, server.item, server.used = capability, item, False
    server.serve_forever()
