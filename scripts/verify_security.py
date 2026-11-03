"""Explicit dependency and configuration audit. Does not prove release readiness.

Contacts npm and RustSec, never novel platforms. Stores lockfile hashes and raw
scanner results; unavailable tools, invalid reports and scanner errors fail closed.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import tarfile
import time


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def npm_result(data, code):
    counts = data.get('metadata', {}).get('vulnerabilities')
    if (code not in (0, 1) or data.get('error') or not isinstance(counts, dict)
            or any(type(counts.get(k)) is not int or counts[k] < 0
                   for k in ('info', 'low', 'moderate', 'high', 'critical', 'total'))):
        raise ValueError('npm audit did not produce a complete vulnerability report')
    if sum(counts[k] for k in ('info', 'low', 'moderate', 'high', 'critical')) != counts['total']:
        raise ValueError('npm vulnerability totals disagree')
    return {'passed': code == 0 and counts['total'] == 0, 'counts': counts}


def rust_result(data, code, active_packages=None):
    findings = data.get('vulnerabilities')
    database = data.get('database')
    settings = data.get('settings', {})
    if (code not in (0, 1) or not isinstance(findings, dict)
            or type(findings.get('count')) is not int or findings['count'] < 0
            or not isinstance(findings.get('list'), list)
            or findings['count'] != len(findings['list'])
            or findings.get('found') != bool(findings['count'])
            or not isinstance(database, dict) or not database.get('last-commit')
            or not isinstance(data.get('warnings'), dict)
            or settings.get('ignore') != [] or settings.get('severity') is not None
            or settings.get('target_arch') != [] or settings.get('target_os') != []):
        raise ValueError('cargo audit did not produce a complete advisory report')
    warnings = []
    for kind, entries in data['warnings'].items():
        for entry in entries:
            package = entry['package']
            active = active_packages is None or (package['name'], package['version']) in active_packages
            warnings.append({'kind': kind, 'package': package['name'], 'version': package['version'],
                             'advisory': (entry.get('advisory') or {}).get('id'), 'in_target_graph': active})
    return {'passed': code == 0 and findings['count'] == 0
            and not any(w['in_target_graph'] and w['kind'] in ('unsound', 'yanked') for w in warnings),
            'vulnerability_count': findings['count'], 'database': database, 'warnings': warnings}


def inspect_config(repo):
    config = json.loads((repo / 'apps/desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    directives = dict((part.strip().split(None, 1) + [''])[:2]
                      for part in config['app']['security']['csp'].split(';') if part.strip())
    for name, value in {'default-src': "'self'", 'script-src': "'self'", 'object-src': "'none'",
                        'base-uri': "'none'", 'frame-src': "'none'", 'form-action': "'none'"}.items():
        if directives.get(name) != value:
            raise ValueError('unexpected main-window CSP: ' + name)
    capabilities = list((repo / 'apps/desktop/src-tauri/capabilities').glob('*.json'))
    if len(capabilities) != 1:
        raise ValueError('capability set changed; review all new permissions')
    capability = json.loads(capabilities[0].read_text(encoding='utf-8'))
    if (capability.get('windows') != ['main'] or capability.get('permissions') != ['core:default']
            or capability.get('remote') or capability.get('webviews')):
        raise ValueError('unexpected remote or native capability grant')
    return {'passed': True, 'scope': 'static main-window CSP and capability grants; not runtime IPC proof'}


def engine_lock(archive, expected, output):
    if archive.stat().st_size != expected['bytes'] or digest(archive) != expected['sha256']:
        raise ValueError('engine source archive is not the pinned distribution')
    with tarfile.open(archive, 'r:gz') as bundle:
        entries = [m for m in bundle if m.name == 'opennovel-engine-source/Cargo.lock']
        if len(entries) != 1 or not entries[0].isfile() or not 0 < entries[0].size <= 2 * 1024 * 1024:
            raise ValueError('missing or invalid engine lockfile')
        # Read one bounded member; never extract paths or execute bundled source.
        with bundle.extractfile(entries[0]) as stream:
            content = stream.read(2 * 1024 * 1024 + 1)
        if len(content) != entries[0].size:
            raise ValueError('engine lockfile size mismatch')
    output.write_bytes(content)
    return output


def run_json(command, output, name, env):
    executable = shutil.which(command[0]) or command[0]
    for attempt in range(1, 4):
        result = subprocess.run([executable, *command[1:]], env=env, capture_output=True, timeout=300)
        (output / (name + f'.attempt-{attempt}.json')).write_bytes(result.stdout)
        (output / (name + f'.attempt-{attempt}.stderr.log')).write_bytes(result.stderr)
        # Retry only a documented advisory database transport failure, never findings.
        if (attempt < 3 and not result.stdout.strip()
                and b"couldn't fetch advisory database" in result.stderr):
            time.sleep(2)
            continue
        (output / (name + '.json')).write_bytes(result.stdout)
        (output / (name + '.stderr.log')).write_bytes(result.stderr)
        if not result.stdout.strip():
            raise ValueError(name + ' scanner failed; inspect ' + name + '.stderr.log')
        return json.loads(result.stdout), result.returncode


def pinned_database(database, expected):
    if not re.fullmatch('[0-9a-f]{40}', expected):
        raise ValueError('full advisory commit required')
    def git(*args):
        return subprocess.check_output(['git', '-C', str(database), *args], timeout=15).decode().strip()
    if git('rev-parse', 'HEAD') != expected or git('status', '--porcelain'):
        raise ValueError('advisory checkout differs from the explicit clean snapshot')
    updated = git('show', '-s', '--format=%cI', 'HEAD')
    age = datetime.datetime.now(datetime.timezone.utc) - datetime.datetime.fromisoformat(updated)
    if not datetime.timedelta(0) <= age <= datetime.timedelta(days=3):
        raise ValueError('pinned advisory snapshot must be at most three days old')
    return {'last-commit': expected, 'last-updated': updated, 'provenance': 'explicit-clean-git-snapshot'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--cargo-audit', default='cargo-audit')
    parser.add_argument('--engine-source', type=pathlib.Path)
    parser.add_argument('--advisory-commit', help='explicit clean RustSec snapshot, at most three days old; does not claim latest network state')
    args = parser.parse_args()
    repo = pathlib.Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if output.exists():
        parser.error('choose a new audit directory; previous evidence is preserved')
    output.mkdir(parents=True)
    os.chdir(repo)
    env = dict(os.environ)
    report = {'version': json.loads((repo / 'package.json').read_text(encoding='utf-8'))['version'],
              'checked_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'passed': False, 'release_ready': False, 'target': 'x86_64-pc-windows-msvc',
              'engine_scanned': False, 'checks': {}, 'inputs': {p: digest(repo / p) for p in ('Cargo.lock', 'package-lock.json')}}
    try:
        report['scanner_version'] = subprocess.check_output([args.cargo_audit, '--version'], env=env, timeout=15).decode().strip()
        report['checks']['configuration'] = inspect_config(repo)
        data, code = run_json(['npm', 'audit', '--include=dev', '--include=optional', '--json', '--registry=https://registry.npmjs.org'], output, 'npm', env)
        report['checks']['npm'] = npm_result(data, code)
        tree = subprocess.check_output(['cargo', 'tree', '--locked', '--offline', '--workspace', '--target',
                                        report['target'], '--prefix', 'none', '--format', '{p}'], env=env, timeout=60)
        (output / 'windows-dependency-tree.txt').write_bytes(tree)
        active = {tuple(line.split()[:2]) for line in tree.decode('utf-8').splitlines() if len(line.split()) >= 2}
        active = {(name, version.removeprefix('v')) for name, version in active}
        command = [args.cargo_audit, 'audit', '--file', str(repo / 'Cargo.lock'), '--db',
                   str(repo / '.cache/rustsec-advisory-db'), '--json']
        snapshot = None
        if args.advisory_commit:
            snapshot = pinned_database(repo / '.cache/rustsec-advisory-db', args.advisory_commit)
            report['advisory_mode'] = 'explicit-pinned-snapshot-not-latest'
            command.append('--no-fetch')
        data, code = run_json(command, output, 'rust', env)
        if snapshot:
            if pinned_database(repo / '.cache/rustsec-advisory-db', args.advisory_commit) != snapshot:
                raise ValueError('advisory snapshot changed during scan')
            data['database'].update(snapshot)
        report['checks']['rust'] = rust_result(data, code, active)
        if args.engine_source:
            lock = json.loads((repo / 'scripts/research/native_engine_lock.json').read_text(encoding='utf-8'))
            archive = args.engine_source.resolve()
            path = engine_lock(archive, lock['files']['corresponding-source.tar.gz'], output / 'engine-Cargo.lock')
            report['inputs']['engine_source_sha256'] = digest(archive)
            report['inputs']['engine_lock_sha256'] = digest(path)
            command[command.index('--file') + 1] = str(path)
            data, code = run_json(command, output, 'engine', env)
            if snapshot:
                if pinned_database(repo / '.cache/rustsec-advisory-db', args.advisory_commit) != snapshot:
                    raise ValueError('advisory snapshot changed during scan')
                data['database'].update(snapshot)
            report['checks']['engine'] = rust_result(data, code)
            report['engine_scanned'] = True
        report['passed'] = all(check['passed'] for check in report['checks'].values())
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, tarfile.TarError) as error:
        report['error'] = str(error)
    (output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, ensure_ascii=True))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
