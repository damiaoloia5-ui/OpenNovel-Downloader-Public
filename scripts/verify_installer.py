"""Install/reinstall/uninstall only on a disposable GitHub-hosted Windows runner.

No local override: this writes installer registration and disposable application
data. It does not prove consumer Windows, missing WebView2, or account acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from verify_project import inspect_native, source_context


PRODUCT = 'OpenNovel Downloader'
IDENTIFIER = 'org.opennovel.downloader'
REGISTRY_KEYS = [rf'Software\Microsoft\Windows\CurrentVersion\Uninstall\{PRODUCT}',
                 rf'Software\opennovel\{PRODUCT}']


def require_hosted_runner(env, platform):
    if platform != 'win32' or env.get('GITHUB_ACTIONS') != 'true' or env.get('RUNNER_ENVIRONMENT') != 'github-hosted':
        raise ValueError('Installer acceptance is restricted to disposable GitHub-hosted Windows runners')
    if env.get('RUNNER_OS') != 'Windows':
        raise ValueError('Windows runner identity is missing')


def contained(path, parent):
    resolved, root = path.resolve(), parent.resolve()
    if resolved == root or not resolved.is_relative_to(root):
        raise ValueError('Path must be strictly inside the designated test root')
    return resolved


def nsis_command(executable, flags, final):
    # NSIS /D and _?= must be last and unquoted, even when their path has spaces.
    # No shell is used. Reject characters that could change argument boundaries.
    if any(c in final for c in '\r\n\0"'):
        raise ValueError('Unsafe NSIS destination')
    return subprocess.list2cmdline([str(executable), *flags]) + ' ' + final


def registered_locations():
    import winreg
    found = []
    for hive in [winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE]:
        for view in [winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY]:
            for name in REGISTRY_KEYS:
                try:
                    with winreg.OpenKey(hive, name, 0, winreg.KEY_READ | view):
                        found.append((hive, view, name))
                except FileNotFoundError:
                    pass
    return found


def verify_registration(destination, version):
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, REGISTRY_KEYS[0], 0,
                        winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
        location = Path(winreg.QueryValueEx(key, 'InstallLocation')[0].strip('"')).resolve()
        installed_version = winreg.QueryValueEx(key, 'DisplayVersion')[0]
    if location != destination or installed_version != version:
        raise ValueError('Installer registration differs from the isolated destination or version')


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_preserved(expected):
    if any(not path.is_file() or sha256(path) != digest for path, digest in expected.items()):
        raise ValueError('Original test library or export was changed or deleted')


def verify_nsis_binary(installed, original):
    # Tauri bundler 2.10.1 patches UNK -> NSS before NSIS compression, then
    # restores the unpatched build output. Compare the entire expected payload,
    # allowing exactly this documented patch, not arbitrary normalized bytes.
    # https://github.com/tauri-apps/tauri/blob/tauri-bundler-v2.10.1/crates/tauri-bundler/src/bundle.rs
    source = original.read_bytes()
    marker = b'__TAURI_BUNDLE_TYPE_VAR_UNK'
    if source.count(marker) != 1:
        raise ValueError('Expected exactly one unpatched Tauri bundle marker')
    expected = source.replace(marker, b'__TAURI_BUNDLE_TYPE_VAR_NSS', 1)
    actual = installed.read_bytes()
    if actual != expected:
        raise ValueError('Installed executable differs beyond the expected NSIS bundle marker')
    return {'original_sha256': hashlib.sha256(source).hexdigest(),
            'installed_sha256': hashlib.sha256(actual).hexdigest(),
            'bundle_marker_offset': source.index(marker),
            'only_expected_nsis_marker_changed': True}


def run(command, output, name):
    with (output / (name + '.log')).open('wb') as stream:
        result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT,
                                timeout=600, creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:
        raise ValueError(f'{name} failed with exit code {result.returncode}; see step log')


def main():
    # These checks precede creating files, launching a process or touching registry.
    require_hosted_runner(os.environ, sys.platform)
    repo = Path(__file__).resolve().parents[1]
    if Path(os.environ['GITHUB_WORKSPACE']).resolve() != repo:
        raise ValueError('Run from the actual checked-out GitHub workspace')
    if registered_locations():
        raise ValueError('Existing OpenNovel registration found; refusing to install over it')
    profile = Path(os.environ['USERPROFILE']).resolve()
    appdata = [contained(Path(os.environ[name]) / IDENTIFIER, profile) for name in ['APPDATA', 'LOCALAPPDATA']]
    if any(path.exists() for path in appdata):
        raise ValueError('Existing application data found; refusing to use it')
    version = json.loads((repo / 'package.json').read_text(encoding='utf-8'))['version']
    installer = repo / f'target/release/bundle/nsis/{PRODUCT}_{version}_x64-setup.exe'
    output = contained(repo / '.artifacts/installer-ci', repo)
    output.mkdir(parents=True, exist_ok=False)
    destination = contained(output / 'Installed App', output)
    report = {'passed': False, 'version': version, 'steps': [],
              'clean_consumer_windows_verified': False, 'webview2_missing_verified': False,
              'native_window_interaction_verified': False, 'account_verified': False,
              'user_library_opened': False, 'real_platform_requested': False}
    report.update(source_context(repo))
    try:
        report['installer_sha256'] = sha256(installer)
        # Make a real schema-valid library from our original two-chapter fixture.
        appdata[0].mkdir(parents=True)
        database = appdata[0] / 'library.db'
        cli = repo / 'target/release/opennovel.exe'
        run([str(cli), '--database', str(database), 'import', str(repo / 'examples/local-book.json')], output, 'seed-library')
        imported = json.loads((output / 'seed-library.log').read_text(encoding='utf-8'))
        if imported.get('state') != 'completed' or imported.get('saved') != 2:
            raise ValueError('Original library fixture is incomplete')
        run([str(cli), '--database', str(database), 'export', imported['book_id'], '--format', 'txt',
             '--output', str(output / 'User Exports')], output, 'seed-export')
        receipt = json.loads((output / 'seed-export.log').read_text(encoding='utf-8'))
        exported = contained(Path(receipt['path']), output / 'User Exports')
        if receipt.get('partial') is not False or receipt.get('saved_chapters') != 2:
            raise ValueError('Original export fixture is incomplete')
        expected = {database: sha256(database), exported: sha256(exported)}
        command = nsis_command(installer, ['/S', '/NS'], '/D=' + str(destination))
        for stage in ['install', 'reinstall']:
            run(command, output, stage)
            executable = contained(destination / 'opennovel-desktop.exe', destination)
            binary = verify_nsis_binary(executable, repo / 'target/release/opennovel-desktop.exe')
            for name, source in [('LICENSE', repo / 'LICENSE'),
                                 ('THIRD_PARTY_NOTICES.txt', repo / '.artifacts/release/THIRD_PARTY_NOTICES.txt')]:
                if sha256(destination / name) != sha256(source):
                    raise ValueError('Installed license material mismatch')
            verify_registration(destination, version)
            stage_output = output / stage
            stage_output.mkdir()
            run([str(executable), '--check-installation', str(stage_output / 'native')], output, stage + '-native')
            inspect_native(stage_output, version)
            verify_preserved(expected)
            report['steps'].append({'name': stage, 'passed': True, 'binary': binary})
        # Run a copy with NSIS _?= so it remains attached and targets only this
        # exact validated install directory. No recursive shell deletion is used.
        uninstaller = output / 'test-uninstall.exe'
        shutil.copy2(destination / 'uninstall.exe', uninstaller)
        run(nsis_command(uninstaller, ['/S'], '_?=' + str(destination)), output, 'uninstall')
        if (destination / 'opennovel-desktop.exe').exists():
            raise ValueError('Uninstall left the main executable installed')
        if any(name == REGISTRY_KEYS[0] for _, _, name in registered_locations()):
            raise ValueError('Uninstall left the application registered')
        verify_preserved(expected)
        report['steps'].append({'name': 'uninstall-preserves-original-library-and-export', 'passed': True})
        report['passed'] = True
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
    finally:
        (output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, ensure_ascii=True))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
