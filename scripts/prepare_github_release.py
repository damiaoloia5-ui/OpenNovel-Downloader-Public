"""Prepare a reviewed local delivery for GitHub Releases; never publish or create tags.

Only explicit, hash-checked delivery files are copied. Logs, databases and local
evidence are not release assets. A completed archive has SHA256SUMS.txt; an error
leaves its new directory for diagnosis and never replaces an earlier archive.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import zipfile


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args])


def prepare(repo, delivery, output):
    repo, delivery, output = map(lambda p: Path(p).resolve(), (repo, delivery, output))
    if output.exists():
        raise ValueError('output already exists; old releases must not be overwritten')
    manifest = json.loads((delivery / 'release-manifest.json').read_text(encoding='utf-8-sig'))
    receipt = json.loads((delivery / 'delivery-check.json').read_text(encoding='utf-8-sig'))
    version, commit = manifest['version'], manifest['source_commit']
    if not re.fullmatch(r'\d+\.\d+\.\d+', version) or not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('invalid release identity')
    source_version = json.loads(git(repo, 'show', f'{commit}:package.json'))['version']
    if source_version != version or receipt.get('passed') is not True or receipt.get('private_library_included') is not False:
        raise ValueError('source version or reviewed delivery receipt mismatch')
    installer = f'OpenNovel Downloader_{version}_x64-setup.exe'
    portable = f'OpenNovel-{version}-Windows.zip'
    files = receipt.get('files', [])
    if len(files) != 2 or {f['name'] for f in files} != {installer, portable}:
        raise ValueError('delivery must contain exactly the two expected artifacts')
    if manifest.get('installer') != installer:
        raise ValueError('installer identity mismatch')
    for entry in files:
        path = delivery / entry['name']
        actual = digest(path)
        if actual != entry['sha256'] or path.stat().st_size != entry['bytes']:
            raise ValueError('delivery artifact changed: ' + entry['name'])
        if entry['name'] == installer and actual != manifest.get('installer_sha256'):
            raise ValueError('installer differs from source manifest')
    lock = json.loads(git(repo, 'show', f'{commit}:scripts/research/native_engine_lock.json'))
    source_name = 'corresponding-source.tar.gz'
    with zipfile.ZipFile(delivery / portable) as archive:
        if archive.testzip() is not None:
            raise ValueError('corrupt portable archive')
        prefix = f'OpenNovel-{version}-Windows/'
        # Read fixed members only; never extract paths supplied by an archive.
        materials = {
            f'OpenNovel-{version}-engine-source.tar.gz': archive.read(prefix + 'opennovel-engine/native-engine/' + source_name),
            'THIRD_PARTY_NOTICES.txt': archive.read(prefix + 'THIRD_PARTY_NOTICES.txt'),
            'LICENSE.txt': archive.read(prefix + 'LICENSE'),
        }
    source = materials[f'OpenNovel-{version}-engine-source.tar.gz']
    if len(source) != lock['files'][source_name]['bytes'] or hashlib.sha256(source).hexdigest() != lock['files'][source_name]['sha256']:
        raise ValueError('engine corresponding source differs from committed lock')
    output.mkdir(parents=True, exist_ok=False)
    for entry in files:
        target = output / entry['name']
        shutil.copyfile(delivery / entry['name'], target)
        if digest(target) != entry['sha256']:
            raise ValueError('artifact changed while copying')
    for name, data in materials.items():
        (output / name).write_bytes(data)
    source_zip = output / f'OpenNovel-{version}-source.zip'
    git(repo, 'archive', '--format=zip', f'--prefix=OpenNovel-{version}/', f'--output={source_zip}', commit)
    with zipfile.ZipFile(source_zip) as archive:
        if archive.testzip() is not None:
            raise ValueError('corrupt source archive')
    record = {
        'schema_version': 1, 'version': version, 'tag': 'v' + version,
        'source_commit': commit, 'package': 'Windows x64 integrated',
        'authenticode_signed': False, 'ready_for_stable_release': False,
        'scope': 'Reviewed local delivery archive; not independent Windows or purchased-content acceptance.',
        'assets': [{'name': p.name, 'bytes': p.stat().st_size, 'sha256': digest(p)}
                   for p in sorted(output.iterdir())],
    }
    (output / 'release.json').write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    notes = f'''# OpenNovel Downloader {version}

Windows x64 一体化版。普通用户下载 `OpenNovel Downloader_{version}_x64-setup.exe`；
便携使用下载 `OpenNovel-{version}-Windows.zip` 并完整解压。不要只复制主程序。
安装包内置 WebView2 离线安装器，便携包需要系统已有 WebView2。
无需另装 Python、Rust、Node 或 Docker；首次启动建立独立空白书库。

本版包括独立本地用户、保存前选择文件夹和章节范围、TXT/EPUB/Word 导出、
小说阅读排版，以及跨进程请求间隔、持久冷却和临时会话清理保护。
番茄本地增强默认关闭，需要用户主动启用。

只保存来源实际开放或本人官网会话实际可见且可识别的内容。
免费、VIP、已登录均不代表全书可保存；不自动购买或处理验证码。
真实已购全书、独立普通 Windows 验收及最终对外许可审核仍未完成。
本包未正式代码签名，不声明全平台完整下载、绝对安全或正式稳定验收完成。

主程序 Apache-2.0；独立引擎保留 GPL-3.0，完整对应源码既在便携包内，
也作为 `OpenNovel-{version}-engine-source.tar.gz` 单独提供。
应用源码固定为 `{commit}`，不会随 main 后续修改而变化。
许可通知见 LICENSE.txt、THIRD_PARTY_NOTICES.txt 和组件内各自许可文件。

下载后可用 PowerShell `Get-FileHash -Algorithm SHA256 <文件路径>` 对照 SHA256SUMS.txt。
校验值用于核对文件一致性，不能替代可信代码签名。
升级前备份书库；卸载默认保留用户数据。

每个版本保留独立 Release、安装包、源码及校验值；修复使用新版本号，不替换旧包。
GitHub Releases 是下载渠道，本地归档也应另存到独立备份介质。
'''
    (output / 'RELEASE_NOTES.md').write_text(notes, encoding='utf-8')
    sums = ''.join(f'{digest(p)}  {p.name}\n' for p in sorted(output.iterdir()))
    (output / 'SHA256SUMS.txt').write_text(sums, encoding='utf-8')
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delivery', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = prepare(Path(__file__).resolve().parents[1], args.delivery, args.output)
    print(json.dumps({'prepared': True, 'version': result['version'], 'source_commit': result['source_commit'],
                      'published': False}, indent=2))


if __name__ == '__main__':
    main()
