Set-StrictMode -Version Latest

function Find-OpenNovelSignTool {
    $candidate = $env:OPENNOVEL_SIGNTOOL
    if (-not $candidate) {
        $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
        if ($command) { $candidate = $command.Source }
    }
    if (-not $candidate) {
        $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
        $candidate = Get-ChildItem -LiteralPath $sdk -Directory -ErrorAction SilentlyContinue |
            Where-Object Name -Match '^\d+\.\d+\.\d+\.\d+$' |
            Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Join-Path $_.FullName 'x64/signtool.exe' } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
            Select-Object -First 1
    }
    if (-not $candidate) { throw 'SignTool is missing. Install the Windows SDK signing tools.' }
    $candidate = (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
    $signature = Get-AuthenticodeSignature -LiteralPath $candidate
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
        throw 'The selected SignTool must have a valid Microsoft signature.'
    }
    return $candidate
}

function Get-OpenNovelSigningCertificate {
    param([string]$Thumbprint, [ValidateSet('CurrentUser','LocalMachine')][string]$StoreLocation = 'CurrentUser')
    if ($Thumbprint -notmatch '^[0-9a-fA-F]{40}$') {
        throw 'Set OPENNOVEL_SIGNING_CERT_THUMBPRINT to the selected CA-issued certificate thumbprint (40 hex characters).'
    }
    $certificate = Get-Item -LiteralPath "Cert:/$StoreLocation/My/$Thumbprint" -ErrorAction Stop
    if ($certificate.Subject -eq $certificate.Issuer) { throw 'Self-issued development certificates are not accepted for this release workflow.' }
    if (-not $certificate.HasPrivateKey) { throw 'The certificate has no accessible private-key provider. Connect the token or sign in to the provider client.' }
    if ($certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -le (Get-Date)) { throw 'The selected signing certificate is not currently valid.' }
    $usages = @($certificate.EnhancedKeyUsageList | ForEach-Object { $_.ObjectId })
    if ($usages -notcontains '1.3.6.1.5.5.7.3.3') { throw 'The selected certificate does not have the Code Signing EKU.' }
    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
    try {
        $chain.ChainPolicy.ApplicationPolicy.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3'))
        $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::Online
        $chain.ChainPolicy.UrlRetrievalTimeout = [TimeSpan]::FromSeconds(15)
        if (-not $chain.Build($certificate)) {
            throw ('Signing certificate chain validation failed: ' + (($chain.ChainStatus | ForEach-Object { $_.Status.ToString() }) -join ', '))
        }
    } finally { $chain.Dispose() }
    return $certificate
}

function Test-OpenNovelTimestampUrl {
    param([string]$Url)
    $uri = $null
    if (-not [uri]::TryCreate($Url, [UriKind]::Absolute, [ref]$uri) -or
        $uri.Scheme -notin @('https','http') -or -not $uri.Host -or
        $uri.UserInfo -or $uri.Query -or $uri.Fragment) {
        throw 'TimestampUrl must be an absolute HTTP(S) RFC3161 endpoint without credentials, query or fragment.'
    }
}

function Confirm-OpenNovelFileSignature {
    param([string]$Path, [string]$SignTool, [string]$ExpectedThumbprint)
    $file = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($file.PSIsContainer -or $file.Extension -ne '.exe') { throw 'Only existing EXE files are accepted.' }
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid') { throw "Authenticode verification failed ($($signature.Status)): $($file.Name)" }
    if ($ExpectedThumbprint -and $signature.SignerCertificate.Thumbprint -ne $ExpectedThumbprint) {
        throw "Unexpected signing certificate: $($file.Name)"
    }
    if (-not $signature.TimeStamperCertificate) { throw "A trusted timestamp is required: $($file.Name)" }
    & $SignTool verify /pa /all /tw /v $file.FullName | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "SignTool verification failed or returned warnings: $($file.Name)" }
    return [ordered]@{
        path = $file.FullName
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        signer = $signature.SignerCertificate.Subject
        certificate_thumbprint = $signature.SignerCertificate.Thumbprint
        timestamp_signer = $signature.TimeStamperCertificate.Subject
        authenticode_status = 'Valid'
    }
}

Export-ModuleMember -Function Find-OpenNovelSignTool,Get-OpenNovelSigningCertificate,Test-OpenNovelTimestampUrl,Confirm-OpenNovelFileSignature
