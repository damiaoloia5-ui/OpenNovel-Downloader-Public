"""Fixed local component layout. No downloads, build commands or account access."""
import hashlib
import json
import os
import pathlib

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = pathlib.Path(__file__).resolve().parent
configured = os.environ.get('OPENNOVEL_ENGINE_ROOT')
if configured:
    PREVIOUS = pathlib.Path(configured)
    if not PREVIOUS.is_absolute() or not PREVIOUS.is_dir():
        raise RuntimeError('absolute prepared component directory required')
else:
    PREVIOUS = REPO / '.artifacts/local-engine-review-20261101'
SOURCE = PREVIOUS / 'source'
OUTPUT = REPO / '.artifacts/local-engine-gate-20261101'
RUST_IMAGE = 'sha256:e18a79fc84dfcfc3ab5ba72290398a644c135c97eaa881447fddc354ee4701a3'
CACHE = 'opennovel-engine-review-20261101-cargo'
TARGET = 'opennovel-engine-review-20261101-target'
CANDIDATE_COMMIT = '07cfb2249d4fd57d19bf4420af0e97621ec92df9'


def verify_source():
    manifest = json.loads((PREVIOUS / 'inventory.json').read_text(encoding='utf-8'))
    if manifest['commit'] != CANDIDATE_COMMIT:
        raise RuntimeError('unapproved candidate revision')
    actual = {p.relative_to(SOURCE).as_posix() for p in SOURCE.rglob('*') if p.is_file()}
    if actual != {entry['path'] for entry in manifest['files']}:
        raise RuntimeError('candidate inventory changed')
    for entry in manifest['files']:
        if hashlib.sha256((SOURCE / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise RuntimeError('candidate bytes changed')
    return manifest
