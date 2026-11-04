"""Offline release evidence ledger. Does not sign, publish, contact accounts or certify safety.

Missing, stale, mismatched or failed evidence blocks readiness. External reports
are reviewer attestations, not facts that this script can independently prove.
"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re

REQUIRED = {
    'engineering': ['full_checks_passed', 'native_runtime_passed', 'resource_allowlist_passed'],
    'security': ['dependency_audit_passed', 'advisory_refresh_succeeded', 'warnings_reviewed'],
    'independent_windows': ['independent_machine', 'standard_user', 'install', 'upgrade', 'restart',
                            'uninstall_preserves_data', 'webview2_bootstrap', 'sleep_resume', 'disk_failure_recovery'],
    'signing': ['authenticode_valid', 'trusted_timestamp', 'uninstaller_verified'],
    'licensing': ['distribution_rights_reviewed', 'corresponding_source_available', 'notices_complete'],
    'platforms': ['declared_scope_verified', 'authorization_reviewed'],
}


def load(path):
    if path.stat().st_size > 1024 * 1024:
        raise ValueError('evidence JSON too large')
    data = json.loads(path.read_text(encoding='utf-8-sig'))
    if not isinstance(data, dict):
        raise ValueError('evidence must be an object')
    return data


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def within(root, value):
    if not isinstance(value, str) or not value or Path(value).is_absolute():
        raise ValueError('expected a relative evidence path')
    path = (root / value).resolve()
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('evidence path missing or outside package root')
    return path


def evaluate(manifest, root, now=None):
    now = now or dt.datetime.now(dt.timezone.utc)
    blockers = []
    try:
        if (manifest.get('schema_version') != 1 or not re.fullmatch(r'[0-9a-f]{40}', manifest.get('source_commit', ''))
                or not isinstance(manifest.get('version'), str) or not manifest['version']
                or not isinstance(manifest.get('sources'), list) or not manifest['sources']
                or any(not isinstance(v, str) or not v for v in manifest['sources'])
                or len(set(manifest['sources'])) != len(manifest['sources'])):
            raise ValueError('invalid release identity or declared source scope')
        artifact = within(root, manifest.get('installer'))
        artifact_hash = digest(artifact)
        if artifact_hash != manifest.get('installer_sha256'):
            raise ValueError('installer hash mismatch')
    except (OSError, ValueError, TypeError) as error:
        return {'ready_for_release_review': False, 'blockers': [str(error)], 'published': False}
    evidence = manifest.get('evidence', {})
    if not isinstance(evidence, dict):
        evidence = {}
    for kind, checks in REQUIRED.items():
        try:
            ref = evidence.get(kind)
            if not isinstance(ref, dict):
                raise ValueError('missing evidence')
            path = within(root, ref.get('path'))
            if digest(path) != ref.get('sha256'):
                raise ValueError('evidence hash mismatch')
            report = load(path)
            if (report.get('kind') != kind or report.get('passed') is not True
                    or any(report.get(k) != manifest[k] for k in ['version', 'source_commit', 'installer_sha256'])):
                raise ValueError('failed evidence or different source/artifact')
            checked = dt.datetime.fromisoformat(report.get('checked_at', ''))
            if checked.tzinfo is None or not dt.timedelta(0) <= now - checked <= dt.timedelta(days=14):
                raise ValueError('missing timezone, stale or future evidence')
            values = report.get('checks', {})
            if not isinstance(values, dict) or any(values.get(k) is not True for k in checks):
                raise ValueError('required checks incomplete')
            if kind == 'platforms' and report.get('verified_sources') != manifest['sources']:
                raise ValueError('verified source scope differs from advertised scope')
            if kind in ('independent_windows', 'licensing', 'platforms') and report.get('evidence_mode') != 'actual':
                raise ValueError('controlled fixtures cannot replace actual acceptance')
        except (OSError, ValueError, TypeError, KeyError) as error:
            blockers.append(kind + ': ' + str(error))
    return {'ready_for_release_review': not blockers, 'blockers': blockers, 'published': False,
            'scope': 'Evidence consistency only; external attestations require human review; not zero-risk certification',
            'installer_sha256': artifact_hash, 'source_commit': manifest['source_commit']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = evaluate(load(args.manifest), args.manifest.resolve().parent)
    with args.output.open('x', encoding='utf-8') as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
    print(json.dumps(report, ensure_ascii=True))
    return 0 if report['ready_for_release_review'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
