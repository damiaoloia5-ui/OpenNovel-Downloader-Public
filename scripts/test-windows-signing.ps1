#requires -Version 7.0
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'windows-signing.psm1') -Force
$root = Split-Path $PSScriptRoot -Parent
$results = [Collections.Generic.List[string]]::new()
function Expect-Failure {
    param([scriptblock]$Work,[string]$Message,[string]$Name)
    try { & $Work; throw "Unexpected success: $Name" } catch {
        if ($_.Exception.Message -notmatch $Message) { throw }
        $results.Add($Name)
    }
}
$signTool = Find-OpenNovelSignTool
$pwsh = (Get-Command pwsh.exe).Source
foreach ($name in @('windows-signing.psm1','windows-sign-file.ps1','windows-signed-release.ps1','test-windows-signing.ps1')) {
    $tokens = $null; $errors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw "PowerShell parse error: $name" }
}
$results.Add('all signing scripts parse')
Expect-Failure { Get-OpenNovelSigningCertificate '' } '40 hex' 'missing certificate rejected'
Expect-Failure { Test-OpenNovelTimestampUrl 'https://user:secret@example.test/timestamp' } 'without credentials' 'credential-bearing timestamp URL rejected'
Test-OpenNovelTimestampUrl 'https://timestamp.digicert.com'
$development = Get-ChildItem Cert:/CurrentUser/My | Where-Object { $_.Subject -eq $_.Issuer -and @($_.EnhancedKeyUsageList | ForEach-Object { $_.ObjectId }) -contains '1.3.6.1.5.5.7.3.3' } | Select-Object -First 1
if ($development) { Expect-Failure { Get-OpenNovelSigningCertificate $development.Thumbprint } 'Self-issued' 'existing self-issued certificate rejected without private-key use' }
$trustedSignature = Get-AuthenticodeSignature -LiteralPath $signTool
$null = Confirm-OpenNovelFileSignature $signTool $signTool $trustedSignature.SignerCertificate.Thumbprint
$results.Add('real timestamped Microsoft binary verifies')
Expect-Failure { Confirm-OpenNovelFileSignature $signTool $signTool ('0'*40) } 'Unexpected signing certificate' 'wrong publisher rejected'
$testDirectory = Join-Path $root ".artifacts/signing-tests/$([guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $testDirectory
$tampered = Join-Path $testDirectory 'modified signed binary.exe'
Copy-Item -LiteralPath $signTool -Destination $tampered
$stream = [IO.File]::Open($tampered,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try { $stream.Position=4096; $value=$stream.ReadByte(); $stream.Position=4096; $stream.WriteByte([byte]($value -bxor 1)) } finally { $stream.Dispose() }
Expect-Failure { Confirm-OpenNovelFileSignature $tampered $signTool '' } 'verification failed' 'modified signed binary rejected'
if ((Get-AuthenticodeSignature -LiteralPath $signTool).Status -ne 'Valid') { throw 'Original Microsoft binary was changed.' }
$candidate = Join-Path $root '.artifacts/v03-release/OpenNovel-Downloader-0.3.0-windows-x64-setup.exe'
if (-not (Test-Path -LiteralPath $candidate)) { throw 'This manual release check requires the preserved v0.3 unsigned candidate.' }
$hash = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash
Expect-Failure { Confirm-OpenNovelFileSignature $candidate $signTool '' } 'NotSigned' 'unsigned OpenNovel candidate rejected'
$beforeDirectories = @(Get-ChildItem -LiteralPath (Join-Path $root '.artifacts/signing-build') -Directory -ErrorAction SilentlyContinue).Count
$entry = Join-Path $PSScriptRoot 'windows-signed-release.ps1'
$failureLog = Join-Path $testDirectory 'missing-certificate.txt'
& $pwsh -NoProfile -File $entry -Action Build -CertificateThumbprint 'invalid' *> $failureLog
if ($LASTEXITCODE -ne 1) { throw 'Missing-certificate build did not return failure.' }
$afterDirectories = @(Get-ChildItem -LiteralPath (Join-Path $root '.artifacts/signing-build') -Directory -ErrorAction SilentlyContinue).Count
if ($afterDirectories -ne $beforeDirectories -or (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash -ne $hash) { throw 'Failed preflight modified candidate or created a build directory.' }
$results.Add('missing certificate fails before build and preserves original installer')
& $pwsh -NoProfile -File (Join-Path $PSScriptRoot 'windows-sign-file.ps1') -TargetPath $candidate -CertificateThumbprint 'invalid' *> (Join-Path $testDirectory 'sign-rejected.txt')
if ($LASTEXITCODE -ne 1 -or (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash -ne $hash) { throw 'Sign-file failure did not preserve original installer.' }
$results.Add('per-file signer refuses missing certificate without modification')
& $pwsh -NoProfile -File $entry -Action Verify -FilePath $candidate -CertificateThumbprint '' *> (Join-Path $testDirectory 'verify-rejected.txt')
if ($LASTEXITCODE -ne 1) { throw 'Unsigned verify command returned success.' }
$results.Add('verify command returns nonzero for unsigned installer')
[ordered]@{ passed=$results.Count;checks=@($results);actual_project_signing='not performed; CA-issued certificate missing';original_installer_sha256=$hash.ToLowerInvariant() } | ConvertTo-Json -Depth 4
