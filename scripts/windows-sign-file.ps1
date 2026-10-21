#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$TargetPath,
    [string]$CertificateThumbprint = $env:OPENNOVEL_SIGNING_CERT_THUMBPRINT,
    [ValidateSet('CurrentUser','LocalMachine')][string]$StoreLocation = 'CurrentUser',
    [string]$TimestampUrl = 'https://timestamp.digicert.com'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'windows-signing.psm1') -Force
try {
    Test-OpenNovelTimestampUrl $TimestampUrl
    $certificate = Get-OpenNovelSigningCertificate $CertificateThumbprint $StoreLocation
    $signTool = Find-OpenNovelSignTool
    $target = Get-Item -LiteralPath $TargetPath
    if ($target.PSIsContainer -or $target.Extension -ne '.exe') { throw 'Only existing EXE files can be signed.' }
    $before = Get-AuthenticodeSignature -LiteralPath $target.FullName
    if ($before.Status -eq 'NotSigned') {
        $signArgs = @('sign','/s','My','/sha1',$certificate.Thumbprint,'/fd','SHA256','/tr',$TimestampUrl,'/td','SHA256','/d','OpenNovel Downloader')
        if ($StoreLocation -eq 'LocalMachine') { $signArgs += '/sm' }
        # Private keys remain in their provider; no passwords or PFX files enter this command.
        & $signTool @signArgs $target.FullName | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Signing failed or returned warnings. The release must not be published.' }
    }
    $record = Confirm-OpenNovelFileSignature $target.FullName $signTool $certificate.Thumbprint
    if ($env:OPENNOVEL_SIGNING_AUDIT) {
        $record | ConvertTo-Json -Compress | Add-Content -LiteralPath $env:OPENNOVEL_SIGNING_AUDIT -Encoding utf8
    }
    $record | ConvertTo-Json -Compress
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
