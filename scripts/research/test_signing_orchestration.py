"""Exercise the real release orchestrator with ORIGINAL fake build/signing tools.

No certificates, private keys, real signatures, network, or actual release builds.
Only a copy of the production orchestrator runs in a new temporary directory.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


MODULE = r'''
function Find-OpenNovelSignTool { return 'original-fake-sign-tool' }
function Test-OpenNovelTimestampUrl { param($Url) }
function Get-OpenNovelSigningCertificate {
    param($Thumbprint,$Store)
    return [pscustomobject]@{ Thumbprint=('A'*40); Subject='ORIGINAL TEST ONLY' }
}
function Confirm-OpenNovelFileSignature {
    param($Path,$Tool,$Thumbprint)
    if (-not ([IO.File]::ReadAllText($Path)).StartsWith('ORIGINAL-FAKE-SIGNED:')) {
        throw ('Original test verifier found unsigned output: '+[IO.Path]::GetFileName($Path))
    }
    return [ordered]@{ path=$Path;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant();test_only=$true }
}
Export-ModuleMember -Function *
'''

SIGNER = r'''
param($TargetPath,$CertificateThumbprint,$StoreLocation,$TimestampUrl)
if ($env:TEST_FAIL_DESKTOP_SIGNING -eq '1' -and [IO.Path]::GetFileName($TargetPath) -eq 'opennovel-desktop.exe') { exit 9 }
[IO.File]::WriteAllText($TargetPath, 'ORIGINAL-FAKE-SIGNED:'+ [IO.File]::ReadAllText($TargetPath))
@{path=$TargetPath;test_only=$true} | ConvertTo-Json -Compress | Add-Content -LiteralPath $env:OPENNOVEL_SIGNING_AUDIT
exit 0
'''

WRAPPER = r'''
$ErrorActionPreference = 'Stop'
function global:git {
    $global:LASTEXITCODE=0
    if ($args[0] -eq 'rev-parse') { return ('b'*40) }
    if ($args[0] -eq 'status') { return }
    throw 'Unexpected git operation in test'
}
function global:cargo {
    $release=Join-Path $env:CARGO_TARGET_DIR 'release'
    $null=New-Item -ItemType Directory -Path $release -Force
    [IO.File]::WriteAllText((Join-Path $release 'opennovel.exe'),'ORIGINAL CLI')
    $global:LASTEXITCODE=0
}
function global:npm.cmd {
    if ($args -contains 'release:notices') {
        $dir=Join-Path $PSScriptRoot '.artifacts/release'
        $null=New-Item -ItemType Directory -Path $dir -Force
        [IO.File]::WriteAllText((Join-Path $dir 'THIRD_PARTY_NOTICES.txt'),'Original test notices')
    } else {
        $release=Join-Path $env:CARGO_TARGET_DIR 'release'
        $bundle=Join-Path $release 'bundle/nsis'
        $null=New-Item -ItemType Directory -Path $bundle -Force
        $prefix=if ($env:TEST_UNSIGNED_INSTALLER -eq '1') { '' } else { 'ORIGINAL-FAKE-SIGNED:' }
        [IO.File]::WriteAllText((Join-Path $bundle 'OpenNovel Downloader_0.0.0_x64-setup.exe'),$prefix+'ORIGINAL INSTALLER')
        # Model Tauri's documented restoration AFTER it signs/packages the main binary.
        [IO.File]::WriteAllText((Join-Path $release 'opennovel-desktop.exe'),'ORIGINAL RESTORED UNSIGNED DESKTOP')
        if ($env:TEST_MISSING_UNINSTALLER -ne '1') {
            @{path=(Join-Path $bundle 'uninstall.exe');test_only=$true} | ConvertTo-Json -Compress | Add-Content -LiteralPath $env:OPENNOVEL_SIGNING_AUDIT
        }
    }
    $global:LASTEXITCODE=0
}
& (Join-Path $PSScriptRoot 'scripts/windows-signed-release.ps1') -Action Build -CertificateThumbprint ('A'*40)
exit $LASTEXITCODE
'''


@unittest.skipUnless(os.name == 'nt', 'Windows PowerShell release orchestration')
class SigningOrchestrationTests(unittest.TestCase):
    def run_release(self, root, **flags):
        shell = shutil.which('pwsh')
        if not shell:
            self.fail('PowerShell 7 required for Windows signing orchestration tests')
        scripts = root / 'scripts'
        scripts.mkdir()
        source = Path(__file__).resolve().parents[1] / 'windows-signed-release.ps1'
        shutil.copy2(source, scripts / source.name)
        shutil.copy2(source.parent / 'windows-installation-check.cmd', scripts / 'windows-installation-check.cmd')
        (scripts / 'windows-signing.psm1').write_text(MODULE, encoding='utf-8')
        (scripts / 'windows-sign-file.ps1').write_text(SIGNER, encoding='utf-8')
        (root / 'wrapper.ps1').write_text(WRAPPER, encoding='utf-8')
        (root / 'package.json').write_text('{"version":"0.0.0"}', encoding='utf-8')
        (root / 'LICENSE').write_text('Original test license', encoding='utf-8')
        env = dict(os.environ, TEST_FAIL_DESKTOP_SIGNING='0', TEST_UNSIGNED_INSTALLER='0', TEST_MISSING_UNINSTALLER='0')
        env.update(flags)
        return subprocess.run([shell, '-NoProfile', '-File', str(root / 'wrapper.ps1')], cwd=root,
                              env=env, capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=45)

    def test_restored_desktop_and_cli_are_signed_before_delivery_hashes(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            result = self.run_release(root)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            receipts = list(root.glob('.artifacts/signed-release/*/manifest.json'))
            self.assertEqual(len(receipts), 1, result.stdout + result.stderr)
            report = json.loads(receipts[0].read_text(encoding='utf-8-sig'))
            self.assertEqual(len(report['signatures']), 3)
            launcher = receipts[0].parent / '运行安装自检.cmd'
            launcher_row = next(item for item in report['files'] if item['name'] == launcher.name)
            self.assertEqual(launcher_row['sha256'], hashlib.sha256(launcher.read_bytes()).hexdigest())
            for name in ['opennovel-desktop.exe', 'opennovel.exe', 'OpenNovel-Downloader-0.0.0-windows-x64-signed-setup.exe']:
                data = (receipts[0].parent / name).read_bytes()
                self.assertTrue(data.startswith(b'ORIGINAL-FAKE-SIGNED:'))
                row = next(item for item in report['files'] if item['name'] == name)
                self.assertEqual(row['sha256'], hashlib.sha256(data).hexdigest())

    def test_desktop_signing_failure_never_creates_delivery(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            result = self.run_release(root, TEST_FAIL_DESKTOP_SIGNING='1')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Executable signing failed: opennovel-desktop.exe', result.stdout + result.stderr)
            self.assertFalse((root / '.artifacts/signed-release').exists())

    def test_unsigned_installer_and_missing_uninstaller_audit_fail_closed(self):
        for flag in ['TEST_UNSIGNED_INSTALLER', 'TEST_MISSING_UNINSTALLER']:
            with self.subTest(flag=flag), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                result = self.run_release(root, **{flag: '1'})
                self.assertNotEqual(result.returncode, 0)
                message = 'Original test verifier found unsigned output:' if flag == 'TEST_UNSIGNED_INSTALLER' else 'No verified uninstaller signature'
                self.assertIn(message, result.stdout + result.stderr)
                self.assertFalse((root / '.artifacts/signed-release').exists())


if __name__ == '__main__':
    unittest.main()
