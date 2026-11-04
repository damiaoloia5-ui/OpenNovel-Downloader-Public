"""Archive a freshly allowlist-verified directory and verify every ZIP entry."""
from pathlib import Path
import hashlib
import json
import sys
import zipfile

root=Path(sys.argv[1]).resolve()
destination=root.with_name(root.name+'.zip')
files=sorted(p for p in root.rglob('*') if p.is_file())
assert files and not any(p.is_symlink() for p in root.rglob('*'))
with zipfile.ZipFile(destination,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
    for path in files: archive.write(path,root.name+'/'+path.relative_to(root).as_posix())
with zipfile.ZipFile(destination) as archive:
    assert archive.testzip() is None
    assert set(archive.namelist())=={root.name+'/'+p.relative_to(root).as_posix() for p in files}
    for path in files:
        assert hashlib.sha256(archive.read(root.name+'/'+path.relative_to(root).as_posix())).digest()==hashlib.sha256(path.read_bytes()).digest()
print(json.dumps({'passed':True,'archive':str(destination),'files':len(files),'sha256':hashlib.sha256(destination.read_bytes()).hexdigest()}))
