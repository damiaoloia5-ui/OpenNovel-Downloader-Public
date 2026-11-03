"""Fail-closed audit contracts with synthetic scanner results and original archives."""
import copy
import hashlib
import io
import json
import pathlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import verify_security as audit


class SecurityAuditTests(unittest.TestCase):
    def test_pinned_database_rejects_changed_dirty_old_or_future_snapshots(self):
        commit = 'a' * 40
        now = audit.datetime.datetime.now(audit.datetime.timezone.utc)
        def command(*args, **kwargs):
            cmd = args[0]
            if 'rev-parse' in cmd: return commit.encode()
            if 'status' in cmd: return b''
            return now.isoformat().encode()
        with patch.object(audit.subprocess, 'check_output', side_effect=command):
            self.assertEqual(audit.pinned_database(pathlib.Path('fixture'), commit)['last-commit'], commit)
        for outputs in [[b'b'*40], [commit.encode(), b' M advisory.md'],
                        [commit.encode(), b'', (now-audit.datetime.timedelta(days=4)).isoformat().encode()],
                        [commit.encode(), b'', (now+audit.datetime.timedelta(days=1)).isoformat().encode()]]:
            with patch.object(audit.subprocess, 'check_output', side_effect=outputs):
                with self.assertRaises(ValueError): audit.pinned_database(pathlib.Path('fixture'), commit)

    def test_only_database_transport_errors_retry_and_evidence_is_preserved(self):
        failed = SimpleNamespace(stdout=b'', stderr=b"couldn't fetch advisory database", returncode=1)
        finding = SimpleNamespace(stdout=b'{"finding":true}', stderr=b'', returncode=1)
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            with patch.object(audit.subprocess, 'run', side_effect=[failed, finding]) as run, patch.object(audit.time, 'sleep'):
                data, code = audit.run_json(['fixture'], root, 'audit', {})
            self.assertEqual((data, code), ({'finding': True}, 1))
            self.assertEqual(run.call_count, 2)
            self.assertTrue((root/'audit.attempt-1.stderr.log').is_file())
            with patch.object(audit.subprocess, 'run', return_value=failed) as run, patch.object(audit.time, 'sleep'):
                with self.assertRaises(ValueError): audit.run_json(['fixture'], root, 'failed', {})
            self.assertEqual(run.call_count, 3)

    def test_scanner_failure_and_missing_results_never_pass(self):
        npm = {'metadata': {'vulnerabilities': dict.fromkeys(['info', 'low', 'moderate', 'high', 'critical', 'total'], 0)}}
        self.assertTrue(audit.npm_result(npm, 0)['passed'])
        self.assertFalse(audit.npm_result(npm, 1)['passed'])
        for value, code in [({}, 0), ({**npm, 'error': 'network'}, 0), (npm, 127)]:
            with self.assertRaises(ValueError): audit.npm_result(value, code)
        bad = copy.deepcopy(npm); bad['metadata']['vulnerabilities']['high'] = 1
        with self.assertRaises(ValueError): audit.npm_result(bad, 0)
        bad['metadata']['vulnerabilities']['total'] = 1
        self.assertFalse(audit.npm_result(bad, 0)['passed'])

    def test_rust_vulnerabilities_and_target_unsoundness_block_even_with_zero_exit(self):
        base = {'database': {'last-commit': 'original-fixture'}, 'settings': {'ignore': [], 'severity': None, 'target_arch': [], 'target_os': []}, 'vulnerabilities': {'count': 0, 'list': [], 'found': False}, 'warnings': {}}
        self.assertTrue(audit.rust_result(base, 0)['passed'])
        for field, value in [('ignore', ['RUSTSEC-EXAMPLE']), ('severity', 'high'), ('target_os', ['linux'])]:
            hidden = copy.deepcopy(base); hidden['settings'][field] = value
            with self.assertRaises(ValueError): audit.rust_result(hidden, 0)
        bad = copy.deepcopy(base); bad['vulnerabilities'] = {'count': 1, 'list': [{}], 'found': True}
        self.assertFalse(audit.rust_result(bad, 0)['passed'])
        for key in ['database', 'vulnerabilities', 'warnings']:
            bad = copy.deepcopy(base); bad.pop(key)
            with self.assertRaises(ValueError): audit.rust_result(bad, 0)
        base['warnings'] = {'unsound': [{'package': {'name': 'original', 'version': '1.0.0'}}]}
        self.assertFalse(audit.rust_result(base, 0, {('original', '1.0.0')})['passed'])
        result = audit.rust_result(base, 0, set())
        self.assertTrue(result['passed']); self.assertFalse(result['warnings'][0]['in_target_graph'])
        base['warnings'] = {'yanked': [{'package': {'name': 'original', 'version': '1.0.0'}, 'advisory': None}]}
        self.assertFalse(audit.rust_result(base, 0)['passed'])
        with self.assertRaises(ValueError): audit.rust_result(base, 127)

    def test_engine_scan_reads_only_the_pinned_bounded_regular_lockfile(self):
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw); archive = root / 'source.tar.gz'; output = root / 'Cargo.lock'
            def make(kind=tarfile.REGTYPE, duplicate=False):
                with tarfile.open(archive, 'w:gz') as bundle:
                    info = tarfile.TarInfo('opennovel-engine-source/Cargo.lock'); info.type = kind
                    info.linkname = '../outside'; info.size = 8 if kind == tarfile.REGTYPE else 0
                    bundle.addfile(info, io.BytesIO(b'original') if info.size else None)
                    if duplicate: bundle.addfile(info, io.BytesIO(b'original'))
                return {'bytes': archive.stat().st_size, 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}
            expected = make(); audit.engine_lock(archive, expected, output)
            self.assertEqual(output.read_bytes(), b'original')
            for kind, duplicate in [(tarfile.SYMTYPE, False), (tarfile.REGTYPE, True)]:
                expected = make(kind, duplicate)
                with self.assertRaises(ValueError): audit.engine_lock(archive, expected, output)
            with self.assertRaises(ValueError): audit.engine_lock(archive, {'bytes': 1, 'sha256': 'wrong'}, output)
            self.assertEqual(output.read_bytes(), b'original')
            self.assertFalse((root / 'outside').exists())

    def test_new_capabilities_or_relaxed_csp_require_review(self):
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw); app = root / 'apps/desktop/src-tauri'; (app/'capabilities').mkdir(parents=True)
            csp = "default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'"
            config = {'app': {'security': {'csp': csp}}}; cap = {'windows': ['main'], 'permissions': ['core:default']}
            config_path = app/'tauri.conf.json'; cap_path = app/'capabilities/default.json'
            config_path.write_text(json.dumps(config)); cap_path.write_text(json.dumps(cap))
            self.assertTrue(audit.inspect_config(root)['passed'])
            cap_path.write_text(json.dumps({**cap, 'remote': {'urls': ['https://example.invalid']}}))
            with self.assertRaises(ValueError): audit.inspect_config(root)
            cap_path.write_text(json.dumps(cap)); config['app']['security']['csp'] = csp.replace("script-src 'self'", "script-src * 'unsafe-eval'")
            config_path.write_text(json.dumps(config))
            with self.assertRaises(ValueError): audit.inspect_config(root)


if __name__ == '__main__': unittest.main()
