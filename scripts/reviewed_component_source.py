"""Recover reviewed upstream source from a pinned local source bundle, without executing it."""
import hashlib
import json
import pathlib
import tarfile

PREFIX = 'opennovel-engine-source/upstream/'
WRAPPER = 'rust/examples/opennovel_engine_service.rs'


def _relative(name):
    path = pathlib.PurePosixPath(name)
    if not name or path.is_absolute() or '..' in path.parts or '\\' in name or ':' in name or path.as_posix() != name:
        raise ValueError('source path refused')
    return path


def _digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify_source(root, inventory):
    source = root / 'source'
    if root.is_symlink() or source.is_symlink() or not source.is_dir():
        raise ValueError('reviewed source missing or linked')
    if json.loads((root / 'inventory.json').read_text(encoding='utf-8')) != inventory:
        raise ValueError('reviewed source inventory changed')
    expected = {item['path']: item for item in inventory['files']}
    paths = list(source.rglob('*'))
    if any(p.is_symlink() for p in paths):
        raise ValueError('source links refused')
    actual = {p.relative_to(source).as_posix(): p for p in paths if p.is_file()}
    if actual.keys() != expected.keys():
        raise ValueError('reviewed source file set changed')
    for name, path in actual.items():
        item = expected[name]
        if path.stat().st_size != item['bytes'] or _digest(path) != item['sha256']:
            raise ValueError('reviewed source bytes changed')
    return source


def recover_source(archive, expected_archive, inventory, destination):
    archive, destination = pathlib.Path(archive), pathlib.Path(destination)
    if not destination.is_absolute():
        raise ValueError('source cache must be absolute')
    expected = {item['path']: item for item in inventory['files']}
    if not expected or len(expected) != len(inventory['files']):
        raise ValueError('invalid reviewed inventory')
    for name in expected:
        _relative(name)
    if archive.is_symlink() or not archive.is_file() or archive.stat().st_size != expected_archive['bytes'] or _digest(archive) != expected_archive['sha256']:
        raise ValueError('corresponding source archive changed')
    if destination.exists():
        return verify_source(destination, inventory)
    recovered = {}
    with tarfile.open(archive, 'r:gz') as source:
        members = source.getmembers()
        if len(members) > 100000 or sum(m.size for m in members) > 512 * 1024 * 1024:
            raise ValueError('source bundle expansion refused')
        for member in members:
            _relative(member.name.rstrip('/') if member.isdir() else member.name)
            if not member.isfile() and not member.isdir():
                raise ValueError('source links or special entries refused')
            if not member.name.startswith(PREFIX) or member.isdir():
                continue
            name = member.name[len(PREFIX):]
            if name == WRAPPER:
                continue
            if name not in expected or name in recovered:
                raise ValueError('unexpected or duplicate reviewed file')
            item = expected[name]
            if member.size != item['bytes']:
                raise ValueError('reviewed file size changed')
            data = source.extractfile(member).read()
            if hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError('reviewed file digest changed')
            recovered[name] = data
    if recovered.keys() != expected.keys():
        raise ValueError('reviewed source incomplete')
    destination.mkdir(parents=True)
    for name, data in recovered.items():
        path = destination / 'source' / _relative(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    (destination / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n', encoding='utf-8')
    return verify_source(destination, inventory)
