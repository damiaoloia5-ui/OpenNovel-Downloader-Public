"""Sequential private mailbox relay; no outbound network or plaintext logging."""
import base64
import hmac
import json
import os
import pathlib
import time
from http.server import HTTPServer

from relay_read_gate import Relay
from https_read_gate import validate_request

MAIL = pathlib.Path('/tmp/opennovel-session')


class PersistentRelay(Relay):
    def handle_one_request(self):
        self.connection.settimeout(3)
        return super().handle_one_request()

    def do_GET(self):
        prefix = '/' + self.server.capability
        if not hmac.compare_digest(self.path[:len(prefix)], prefix) or self.server.halted:
            return self.reply(403)
        try:
            approval = json.loads((MAIL / 'approval.json').read_text())
            sequence = approval['sequence']
            if sequence != self.server.previous + 1:
                return self.reply(403)
            request = {'method': 'GET', 'target': self.path[len(prefix):], 'headers': list(self.headers.raw_items())}
            validate_request(request, approval['item_id'])
        except (OSError, ValueError, KeyError, TypeError):
            return self.reply(403)
        self.server.previous = sequence
        (MAIL / f'request-{sequence}.json').write_text(json.dumps(request))
        deadline = time.monotonic() + 25
        response = MAIL / f'response-{sequence}.json'
        while time.monotonic() < deadline:
            if response.exists():
                result = json.loads(response.read_text())
                if result['status'] != 200:
                    self.server.halted = True
                self.reply(result['status'], result['headers'], base64.b64decode(result['body'], validate=True))
                response.unlink()
                (MAIL / f'request-{sequence}.json').unlink()
                return
            time.sleep(0.02)
        self.server.halted = True
        self.reply(504)


if __name__ == '__main__':
    cap = os.environ.get('OPENNOVEL_REVIEW_CAPABILITY', '')
    if len(cap) != 64 or any(c not in '0123456789abcdef' for c in cap):
        raise SystemExit('private capability required')
    MAIL.mkdir(mode=0o700)
    server = HTTPServer(('127.0.0.1', 8123), PersistentRelay)
    server.capability, server.previous, server.halted = cap, 0, False
    server.timeout = 1
    # Host failure cannot leave an idle broker alive indefinitely.
    while True:
        server.last_request = time.monotonic()
        deadline = server.last_request + 90
        while time.monotonic() < deadline:
            before = server.previous
            server.handle_request()
            if server.previous != before:
                break
        else:
            break
