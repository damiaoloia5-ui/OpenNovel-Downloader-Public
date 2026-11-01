"""Pinned offline component. No network downloads, extraction, Docker installation or pruning."""
import hashlib
import json
import pathlib
import re
import subprocess

import engine_runtime_config as sandbox

CREATE_FLAGS = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
FILES = {'runtime-image.tar.gz', 'corresponding-source.tar.gz'}


class BundleError(RuntimeError):
    def __init__(self, reason='offline_bundle_invalid'):
        super().__init__(reason)
        self.reason = reason


def present():
    return (sandbox.PREVIOUS / 'offline-engine').exists()


def _lock():
    # The application verifies this file against its compiled inventory before
    # executing any worker. The distribution manifest cannot authorize code.
    raw = pathlib.Path(__file__).with_name('engine_bundle_lock.json').read_bytes()
    if len(raw) > 16384:
        raise BundleError()
    lock = json.loads(raw)
    if lock.get('schema_version') != 1 or not re.fullmatch(r'sha256:[0-9a-f]{64}', lock.get('image_id', '')):
        raise BundleError()
    if lock.get('binary_path') != '/opt/opennovel/opennovel_engine_service' or set(lock.get('files', {})) != FILES:
        raise BundleError()
    return raw, lock


def verify():
    try:
        raw, lock = _lock()
        root = sandbox.PREVIOUS / 'offline-engine'
        entries = list(root.iterdir())
        if {p.name for p in entries} != FILES | {'manifest.json'} or any(not p.is_file() or p.is_symlink() for p in entries):
            raise BundleError()
        if (root / 'manifest.json').read_bytes() != raw:
            raise BundleError()
        for name, expected in lock['files'].items():
            path = root / name
            if type(expected.get('bytes')) is not int or not 0 < expected['bytes'] <= 300 * 1024 * 1024:
                raise BundleError()
            if path.stat().st_size != expected['bytes']:
                raise BundleError()
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                while chunk := stream.read(1024 * 1024):
                    digest.update(chunk)
            if digest.hexdigest() != expected.get('sha256'):
                raise BundleError()
        return lock
    except (OSError, ValueError, KeyError, TypeError):
        raise BundleError() from None


def image_ready(lock):
    try:
        result = subprocess.run([sandbox.DOCKER, 'image', 'inspect', lock['image_id']],
                                check=True, capture_output=True, timeout=8, creationflags=CREATE_FLAGS)
        values = json.loads(result.stdout)
        if len(values) != 1:
            raise BundleError()
        image = values[0]
        if image['Id'] != lock['image_id'] or image['Os'] != 'linux' or image['Architecture'] != 'amd64' or image['Config']['User'] != '10001:10001':
            raise BundleError()
        return True
    except (OSError, subprocess.SubprocessError):
        return False
    except (ValueError, KeyError, TypeError):
        raise BundleError() from None


def install():
    lock = verify()
    if image_ready(lock):
        return lock
    try:
        subprocess.run([sandbox.DOCKER, 'image', 'load', '--quiet', '--input',
                        str(sandbox.PREVIOUS / 'offline-engine/runtime-image.tar.gz')],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       timeout=120, creationflags=CREATE_FLAGS)
    except (OSError, subprocess.SubprocessError):
        raise BundleError('offline_import_failed') from None
    if not image_ready(lock):
        raise BundleError('offline_import_failed')
    return lock
