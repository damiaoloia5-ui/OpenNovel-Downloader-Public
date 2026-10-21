#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('Inspect','Build','Verify')][string]$Action = 'Inspect',
    [string]$CertificateThumbprint = $env:OPENNOVEL_SIGNING_CERT_THUMBPRINT,
    [ValidateSet('CurrentUser','LocalMachine')][string]$StoreLocation = 'CurrentUser',
    [string]$TimestampUrl = 'https://timestamp.digicert.com',
    [string]$FilePath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'windows-signing.psm1') -Force
$root = Split-Path $PSScriptRoot -Parent
try {
    $signTool = Find-OpenNovelSignTool
    if ($Action -eq 'Verify') {
        if (-not $FilePath) { throw 'Verify requires -FilePath.' }
        Confirm-OpenNovelFileSignature $FilePath $signTool $CertificateThumbprint | ConvertTo-Json
        exit 0
    }
    Test-OpenNovelTimestampUrl $TimestampUrl
    if ($Action -eq 'Inspect') {
        $reason = ''
        $ready = $false
        try { $null = Get-OpenNovelSigningCertificate $CertificateThumbprint $StoreLocation; $ready = $true } catch { $reason = $_.Exception.Message }
        [ordered]@{ sign_tool = $signTool; certificate_ready = $ready; reason = $reason; store = $StoreLocation; timestamp_url = $TimestampUrl; files_modified = $false } | ConvertTo-Json
        exit 0
    }
    # Stop before creating any build/release directory when identity or trust is missing.
    $certificate = Get-OpenNovelSigningCertificate $CertificateThumbprint $StoreLocation
    Push-Location $root
    try {
        $dirty = & git status --porcelain
        if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Signed releases require a clean, committed source tree.' }
        $commit = (& git rev-parse HEAD).Trim()
        $version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
        $run = "$version-$([guid]::NewGuid().ToString('N'))"
        $working = Join-Path $root ".artifacts/signing-build/$run"
        $delivery = Join-Path $root ".artifacts/signed-release/$run"
        $null = New-Item -ItemType Directory -Path $working
        $audit = Join-Path $working 'signing-audit.jsonl'
        $signer = Join-Path $PSScriptRoot 'windows-sign-file.ps1'
        $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
        $configuration = @{ bundle = @{ windows = @{ signCommand = @{
            cmd = $pwsh
            args = @('-NoProfile','-File',$signer,'-CertificateThumbprint',$certificate.Thumbprint,'-StoreLocation',$StoreLocation,'-TimestampUrl',$TimestampUrl,'-TargetPath','%1')
        } } } }
        $configPath = Join-Path $working 'tauri.signing.conf.json'
        $configuration | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $configPath -Encoding utf8
        $oldTarget = $env:CARGO_TARGET_DIR
        $oldAudit = $env:OPENNOVEL_SIGNING_AUDIT
        try {
            $env:CARGO_TARGET_DIR = Join-Path $working 'target'
            $env:OPENNOVEL_SIGNING_AUDIT = $audit
            & cargo build --locked --release -p opennovel-cli
            if ($LASTEXITCODE -ne 0) { throw 'CLI build failed.' }
            & npm.cmd run release:notices
            if ($LASTEXITCODE -ne 0) { throw 'License generation failed.' }
            & npm.cmd run tauri --workspace '@opennovel/desktop' -- build --config src-tauri/tauri.installer.conf.json --config $configPath --bundles nsis
            if ($LASTEXITCODE -ne 0) { throw 'Signed desktop packaging failed.' }
            $cli = Join-Path $env:CARGO_TARGET_DIR 'release/opennovel.exe'
            & $pwsh -NoProfile -File $signer -TargetPath $cli -CertificateThumbprint $certificate.Thumbprint -StoreLocation $StoreLocation -TimestampUrl $TimestampUrl
            if ($LASTEXITCODE -ne 0) { throw 'CLI signing failed.' }
            $desktop = Join-Path $env:CARGO_TARGET_DIR 'release/opennovel-desktop.exe'
            $installer = Join-Path $env:CARGO_TARGET_DIR "release/bundle/nsis/OpenNovel Downloader_${version}_x64-setup.exe"
            $verified = @($desktop,$installer,$cli) | ForEach-Object { Confirm-OpenNovelFileSignature $_ $signTool $certificate.Thumbprint }
            $auditRecords = @(Get-Content -LiteralPath $audit | ForEach-Object { $_ | ConvertFrom-Json })
            if (-not ($auditRecords | Where-Object { [IO.Path]::GetFileName($_.path) -match '(?i)uninstall.*\.exe$' })) {
                throw 'No verified uninstaller signature was observed. Do not publish this package.'
            }
            if ((& git rev-parse HEAD).Trim() -ne $commit -or (& git status --porcelain)) { throw 'Source tree changed during signing. Do not publish this package.' }
            # Final output is separate from all earlier unsigned candidates, with hashes computed after signing.
            $null = New-Item -ItemType Directory -Path $delivery
            Copy-Item -LiteralPath $installer -Destination (Join-Path $delivery "OpenNovel-Downloader-$version-windows-x64-signed-setup.exe")
            Copy-Item -LiteralPath $cli -Destination (Join-Path $delivery 'opennovel.exe')
            Copy-Item -LiteralPath $desktop -Destination (Join-Path $delivery 'opennovel-desktop.exe')
            Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $delivery
            Copy-Item -LiteralPath (Join-Path $root '.artifacts/release/THIRD_PARTY_NOTICES.txt') -Destination $delivery
            $files = @(Get-ChildItem -LiteralPath $delivery -File | ForEach-Object { [ordered]@{ name=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() } })
            [ordered]@{ project='OpenNovel Downloader';version=$version;source_commit=$commit;status='signed_candidate_requires_runtime_and_release_gate_acceptance';signer=$certificate.Subject;certificate_thumbprint=$certificate.Thumbprint;signatures=$verified;uninstaller_signature_observed=$true;files=$files } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $delivery 'manifest.json') -Encoding utf8
            $checksumLines = @(Get-ChildItem -LiteralPath $delivery -File | Where-Object Name -NE 'SHA256SUMS.txt' | Sort-Object Name | ForEach-Object { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()+'  '+$_.Name })
            $checksumLines | Set-Content -LiteralPath (Join-Path $delivery 'SHA256SUMS.txt') -Encoding utf8
            Write-Output "Verified signed candidate: $delivery"
        } finally { $env:CARGO_TARGET_DIR = $oldTarget; $env:OPENNOVEL_SIGNING_AUDIT = $oldAudit }
    } finally { Pop-Location }
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
