"""Run controlled project checks and write a fresh report. Never requests novel platforms."""
import argparse
import datetime
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import uuid


def build_steps(repo, package, native, offline, distribution=None, python_archive=None):
    locked = ['--locked', *(['--offline'] if offline else [])]
    steps = [('format', ['cargo', 'fmt', '--all', '--check']),
             ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', *locked, '--', '-D', 'warnings']),
             ('rust-tests', ['cargo', 'test', '--workspace', *locked]),
             ('lint', ['npm', 'run', 'lint']),
             ('typecheck', ['npm', 'run', 'typecheck']),
             ('frontend-tests', ['npm', 'run', 'test:frontend']),
             ('python-tests', [sys.executable, '-W', 'error', '-m', 'unittest', 'discover', '-s', 'scripts/research', '-p', 'test_*.py']),
             ('frontend-build', ['npm', 'run', 'build'])]
    if os.name == 'nt':
        steps.append(('signing-contracts', ['npm', 'run', 'signing:test']))
    if package == 'integrated':
        if distribution is None:
            raise ValueError('integrated packaging requires --distribution')
        command = [sys.executable, str(repo / 'scripts/prepare_integrated_component.py'), '--distribution', str(distribution)]
        if python_archive is not None:
            command += ['--python-archive', str(python_archive)]
        steps.append(('integrated-resources', command))
    steps.append(('cli-build', ['cargo', 'build', '-p', 'opennovel-cli', '--release', *locked]))
    steps.append(('cli-original', [sys.executable, str(repo / 'scripts/verify_cli.py'), '--executable',
                                   str(repo / 'target/release' / ('opennovel.exe' if os.name == 'nt' else 'opennovel'))]))
    steps.append(('desktop-build', ['npm', 'run', {'none': 'desktop:build', 'basic': 'desktop:package',
                                                 'integrated': 'desktop:package:integrated'}[package]]))
    if native:
        steps.append(('native-installation', [str(repo / 'target/release/opennovel-desktop.exe'), '--check-installation']))
    return steps


def run_step(name, command, repo, output, env):
    executable = shutil.which(command[0]) or command[0]
    log = output / (name + '.log')
    print('START ' + name, flush=True)
    started = time.monotonic()
    with log.open('wb') as stream:
        try:
            code = subprocess.call([executable, *command[1:]], cwd=repo, env=env,
                                   stdout=stream, stderr=subprocess.STDOUT)
        except OSError as error:
            stream.write(('Required command could not start: ' + str(error)).encode('utf-8'))
            code = 127
    result = {'name': name, 'passed': code == 0, 'exit_code': code,
              'elapsed_seconds': round(time.monotonic() - started, 3), 'log': log.name}
    print(('PASS ' if code == 0 else 'FAIL ') + name, flush=True)
    return result


def inspect_native(output, version):
    reports = list((output / 'native').glob('check-*/report.json'))
    if len(reports) != 1:
        raise ValueError('expected exactly one fresh native report')
    report = json.loads(reports[0].read_text(encoding='utf-8'))
    if (report.get('version') != version or report.get('required_checks_passed') is not True
            or report.get('deleted_sample_txt_recovered') is not True
            or report.get('user_library_opened') is not False
            or report.get('real_platform_requested') is not False
            or report.get('native_window_interaction_checked') is not False):
        raise ValueError('native evidence is missing, stale or outside controlled scope')
    return str(reports[0].relative_to(output))


def check_runtime():
    requirements = [('node', ['node', '--version'], (22, 12)), ('rustc', ['rustc', '--version'], (1, 96))]
    versions = {'python': '.'.join(map(str, sys.version_info[:3]))}
    if sys.version_info < (3, 11):
        raise ValueError('Python >=3.11 required')
    for name, command, minimum in requirements:
        raw = subprocess.check_output(command, text=True).strip()
        match = re.search(r'(\d+)\.(\d+)\.(\d+)', raw)
        if not match or tuple(map(int, match.groups()[:2])) < minimum:
            raise ValueError(name + ' does not meet the documented minimum')
        versions[name] = match.group(0)
    if not shutil.which('npm') or not shutil.which('cargo'):
        raise ValueError('npm and cargo are required; install documented prerequisites')
    return versions


def source_context(repo):
    unknown = {'source_commit': None, 'source_dirty': None, 'source_provenance': 'git-metadata-unavailable'}
    if not (repo / '.git').exists():
        return unknown
    try:
        revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True, capture_output=True)
        if revision.returncode == 0:
            dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo))
            return {'source_commit': revision.stdout.strip(), 'source_dirty': dirty, 'source_provenance': 'git'}
    except OSError:
        pass
    # Source ZIPs do not contain .git. Unknown provenance is explicit; no init,
    # inferred clean status or invented commit is allowed.
    return unknown


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=pathlib.Path, help='new output directory; never overwrite earlier evidence')
    parser.add_argument('--package', choices=['none', 'basic', 'integrated'], default='none')
    parser.add_argument('--native', action='store_true', help='run the newly built native original-content installation check')
    parser.add_argument('--offline', action='store_true', help='forbid cargo network fetches; existing caches required')
    parser.add_argument('--distribution', type=pathlib.Path, help='pinned offline engine bundle for integrated packaging')
    parser.add_argument('--python-archive', type=pathlib.Path, help='pinned Python ZIP for integrated packaging')
    args = parser.parse_args()
    if args.package != 'integrated' and (args.distribution or args.python_archive):
        parser.error('component inputs require --package integrated')
    if args.package == 'integrated' and not args.distribution:
        parser.error('--package integrated requires --distribution')
    if args.native and os.name != 'nt':
        parser.error('native Windows acceptance requires Windows')
    repo = pathlib.Path(__file__).resolve().parents[1]
    output = (args.output or repo / '.artifacts/checks' / (datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ-') + uuid.uuid4().hex[:8])).resolve()
    if output.exists():
        parser.error('output already exists; choose a new directory to preserve evidence')
    output.mkdir(parents=True)
    report = {'schema_version': 1, 'version': json.loads((repo / 'package.json').read_text(encoding='utf-8'))['version'],
              'passed': False, 'steps': [], 'native_window_interaction_verified': False,
              'clean_independent_windows_verified': False, 'real_account_entitlement_verified': False,
              'real_platform_requested': False}
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
    temporary = repo / '.cache/verification-temp'
    temporary.mkdir(parents=True, exist_ok=True)
    env['TMP'] = env['TEMP'] = str(temporary)
    if args.offline:
        env['CARGO_NET_OFFLINE'] = 'true'
    try:
        report['runtimes'] = check_runtime()
        report.update(source_context(repo))
        if not (repo / 'node_modules').is_dir():
            raise ValueError('node_modules missing; run npm ci before verification')
        for name, command in build_steps(repo, args.package, args.native, args.offline,
                                         args.distribution.resolve() if args.distribution else None,
                                         args.python_archive.resolve() if args.python_archive else None):
            if name == 'native-installation':
                command.append(str(output / 'native'))
            if name == 'cli-original':
                command += ['--version', report['version'], '--output', str(output / 'cli')]
            result = run_step(name, command, repo, output, env)
            report['steps'].append(result)
            if not result['passed']:
                break
            if name == 'native-installation':
                report['native_report'] = inspect_native(output, report['version'])
        else:
            report['passed'] = True
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
    finally:
        (output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'passed': report['passed'], 'report': str(output / 'report.json')}, ensure_ascii=False), flush=True)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
