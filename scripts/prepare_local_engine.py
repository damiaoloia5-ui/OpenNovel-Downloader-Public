"""Prepare a separate local component from reviewed files; never download or run the candidate."""
import argparse
import hashlib
import json
import pathlib
import shutil
import sys

RUNTIME_FILES = ('desktop_engine_worker.py', 'engine_runtime_config.py', 'persistent_engine.py',
                 'persistent_read_gate.py', 'relay_read_gate.py', 'https_read_gate.py', 'local_engine_gate.py',
                 'offline_engine_bundle.py', 'engine_bundle_lock.json',
                 'native_engine.py', 'native_engine_lock.json', 'windows_engine_sandbox.py')


def assemble(destination, source, inventory, scripts, license_file, executable=None):
    destination = pathlib.Path(destination)
    if not destination.is_absolute():
        raise ValueError('output must be an absolute new directory')
    if destination.exists():
        raise FileExistsError('refusing to overwrite an existing component')
    # Check all required inputs before creating anything. A failed partial copy
    # is retained for diagnosis; never recursively delete a user-selected path.
    required = [inventory, license_file, source / 'LICENSE', *[scripts / x for x in RUNTIME_FILES]]
    if executable is not None:
        required.append(executable)
    if any(not p.is_file() for p in required) or not source.is_dir():
        raise ValueError('required reviewed input is missing')
    destination.mkdir(parents=True)
    shutil.copytree(source, destination / 'source')
    shutil.copy2(inventory, destination / 'inventory.json')
    shutil.copy2(license_file, destination / 'OPENNOVEL-LICENSE.txt')
    runtime = destination / 'runtime'
    runtime.mkdir()
    for name in RUNTIME_FILES:
        shutil.copy2(scripts / name, runtime / name)
    manifest = {'schema_version': 1, 'candidate_commit': json.loads(inventory.read_text(encoding='utf-8'))['commit'],
                'runtime_sha256': {name: hashlib.sha256((runtime / name).read_bytes()).hexdigest() for name in RUNTIME_FILES},
                'inventory_sha256': hashlib.sha256(inventory.read_bytes()).hexdigest(),
                'python': sys.executable, 'engine_binary_bundled': False, 'runtime_ready_checked': False}
    if executable is not None:
        shutil.copy2(executable, destination / 'OpenNovel-Preview.exe')
        manifest['application_sha256'] = hashlib.sha256((destination / 'OpenNovel-Preview.exe').read_bytes()).hexdigest()
        # Components are local preparation output, not a public installer.
        lines = ['@echo off', f'set "OPENNOVEL_ENGINE_PYTHON={sys.executable}"',
                 'set "OPENNOVEL_ENGINE_ROOT=%~dp0"',
                 r'start "" "%~dp0OpenNovel-Preview.exe" --database "%~dp0preview-library\library.db"']
        (destination / 'Start-Isolated-Preview.cmd').write_bytes(('\r\n'.join(lines) + '\r\n').encode('utf-8'))
        checks = ['@echo off', f'set "OPENNOVEL_ENGINE_PYTHON={sys.executable}"',
                  'set "OPENNOVEL_ENGINE_ROOT=%~dp0"',
                  r'start "" /wait "%~dp0OpenNovel-Preview.exe" --check-installation "%~dp0installation-checks"',
                  r'if exist "%~dp0installation-checks" start "" "%~dp0installation-checks"']
        (destination / 'Check-Installation.cmd').write_bytes(('\r\n'.join(checks) + '\r\n').encode('utf-8'))
    (destination / 'component.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=pathlib.Path, required=True, help='absolute new component directory')
    parser.add_argument('--executable', type=pathlib.Path, help='optional desktop EXE to copy with an isolated-library launcher')
    args = parser.parse_args()
    repo = pathlib.Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo / 'scripts/research'))
    import engine_runtime_config as config
    config.verify_source()
    manifest = assemble(args.output, config.SOURCE, config.PREVIOUS / 'inventory.json',
                        config.SCRIPTS, repo / 'LICENSE', args.executable)
    print(json.dumps({'prepared': True, 'output': str(args.output), 'candidate_commit': manifest['candidate_commit'],
                      'engine_binary_bundled': False, 'runtime_ready_checked': False}))


if __name__ == '__main__':
    main()
