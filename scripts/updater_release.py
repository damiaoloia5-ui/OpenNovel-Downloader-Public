"""Local Windows maintainer tool. Keys are DPAPI protected; no network or publishing.

Update signatures are separate from Windows Authenticode certificates. Keep the
DPAPI key with this Windows user's recovery material; never add it to Git/Release.
"""
import argparse
import base64
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
KEY = ROOT / '.artifacts/private-updater/signing-key.dpapi'
PUBLIC_KEY = ROOT / 'apps/desktop/src-tauri/updater-public-key.txt'
RELEASES = 'https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/download'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def protect(data, decrypt=False):
    if os.name != 'nt':
        raise ValueError('Signing key storage requires this Windows user and DPAPI')
    class Blob(ctypes.Structure):
        _fields_ = [('size', wintypes.DWORD), ('data', ctypes.POINTER(ctypes.c_ubyte))]
    buffer = (ctypes.c_ubyte * len(data)).from_buffer_copy(data)
    source, target = Blob(len(data), buffer), Blob()
    crypt = ctypes.WinDLL('crypt32', use_last_error=True)
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    function = crypt.CryptUnprotectData if decrypt else crypt.CryptProtectData
    function.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    function.restype = wintypes.BOOL
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if not function(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(target)):
        raise ValueError('DPAPI failed; use the Windows user that created the signing key')
    try:
        return ctypes.string_at(target.data, target.size)
    finally:
        kernel.LocalFree(target.data)


def signer(args, env=None):
    # Capture output: generate/sign may print private key material. Never forward it.
    result = subprocess.run(['node', str(ROOT / 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', *args],
                            env=env, capture_output=True, cwd=ROOT, timeout=180)
    if result.returncode:
        raise ValueError('Tauri signer failed; no private signer output is logged')


def init_key():
    if KEY.exists() or PUBLIC_KEY.exists():
        raise ValueError('Signing identity already exists; refusing to rotate an installed public key')
    KEY.parent.mkdir(parents=True, exist_ok=True)
    password = secrets.token_urlsafe(48)
    with tempfile.TemporaryDirectory(prefix='key-generation-', dir=KEY.parent) as directory:
        key = Path(directory) / 'key'
        signer(['generate', '--ci', '--password', password, '--write-keys', str(key)])
        public = key.with_suffix('.pub').read_text(encoding='utf-8').strip()
        payload = {'private_key': key.read_text(encoding='utf-8'), 'password': password, 'public_key': public}
        protected = protect(json.dumps(payload).encode())
        assert json.loads(protect(protected, decrypt=True)) == payload
        with KEY.open('xb') as stream:
            stream.write(protected)
            stream.flush()
            os.fsync(stream.fileno())
        with PUBLIC_KEY.open('x', encoding='utf-8') as stream:
            stream.write(public + '\n')
    print('Updater public key saved; private key is protected by this Windows user. No key material printed.')


def manifest(version, signature):
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('Stable numeric version required')
    # A trusted version comment is covered by minisign's global signature.
    try:
        decoded = base64.b64decode(signature, validate=True).decode('utf-8')
    except (ValueError, UnicodeError) as error:
        raise ValueError('Invalid signature encoding') from error
    comments = [line.removeprefix('trusted comment: ') for line in decoded.splitlines() if line.startswith('trusted comment: ')]
    if len(comments) != 1 or f'version:{version}' not in comments[0].split('\t'):
        raise ValueError('Signer did not bind the requested app version')
    return {'version': version, 'notes': '更新应用并保留现有本地书库。安装前会自动生成书库备份。',
            'platforms': {'windows-x86_64': {'url': f'{RELEASES}/v{version}/OpenNovel-{version}-Windows-x64-setup.exe',
                                            'signature': signature}}}


def prepare(installer, privacy_report, output, verifier):
    installer, output = Path(installer).resolve(), Path(output).resolve()
    if output.exists():
        raise ValueError('Output exists; signed release assets must never be overwritten')
    version = json.loads((ROOT / 'package.json').read_text(encoding='utf-8'))['version']
    privacy = json.loads(Path(privacy_report).read_text(encoding='utf-8'))
    if (privacy.get('passed') is not True or privacy.get('private_library_included') is not False
            or privacy.get('installer_sha256') != digest(installer) or privacy.get('version') != version
            or privacy.get('installer_payload_files', 0) < 1000):
        raise ValueError('A matching, allowlisted integrated installer privacy receipt is required')
    key = json.loads(protect(KEY.read_bytes(), decrypt=True))
    public = PUBLIC_KEY.read_text(encoding='utf-8').strip()
    config = json.loads((ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    if key['public_key'] != public or config['plugins']['updater']['pubkey'] != public:
        raise ValueError('Private key does not match the public key compiled into this application')
    output.mkdir(parents=True, exist_ok=False)
    target = output / f'OpenNovel-{version}-Windows-x64-setup.exe'
    shutil.copyfile(installer, target)
    if digest(target) != privacy['installer_sha256']:
        raise ValueError('Installer changed while copying')
    env = dict(os.environ, TAURI_SIGNING_PRIVATE_KEY=key['private_key'], TAURI_SIGNING_PRIVATE_KEY_PASSWORD=key['password'])
    signer(['sign', '--app-version', version, str(target)], env=env)
    signature = Path(str(target) + '.sig').read_text(encoding='utf-8').strip()
    latest = manifest(version, signature)
    (output / 'latest.json').write_text(json.dumps(latest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    # Invoke the actual packaged executable, not a separate key supplied by the caller.
    verifier = Path(verifier).resolve()
    if digest(verifier) != digest(ROOT / 'target/release/opennovel-desktop.exe'):
        raise ValueError('Verifier must match the current packaged application')
    checked = subprocess.run([str(verifier), '--verify-update', str(output / 'latest.json'), str(target)],
                             capture_output=True, timeout=120)
    if checked.returncode:
        raise ValueError('Packaged application rejected this update signature; do not publish')
    proof = {'passed': True, 'version': version, 'installer_sha256': digest(target),
             'manifest_sha256': digest(output / 'latest.json'), 'verifier_sha256': digest(verifier)}
    (output / 'verification.json').write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'prepared': True, 'version': version, 'installer_sha256': digest(target), 'published': False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('init-key')
    prepare_cmd = sub.add_parser('prepare')
    prepare_cmd.add_argument('--installer', required=True, type=Path)
    prepare_cmd.add_argument('--privacy-report', required=True, type=Path)
    prepare_cmd.add_argument('--output', required=True, type=Path)
    prepare_cmd.add_argument('--verifier', required=True, type=Path)
    args = parser.parse_args()
    if args.command == 'init-key':
        init_key()
    else:
        prepare(args.installer, args.privacy_report, args.output, args.verifier)


if __name__ == '__main__':
    main()
