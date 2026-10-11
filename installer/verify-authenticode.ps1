param(
    [Parameter(Mandatory = $true)]
    [string] $Path
)

$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $Path
if ($signature.Status -ne 'Valid') {
    Write-Error $signature.Status
    exit 2
}

$subject = $signature.SignerCertificate.Subject
if ($subject -notmatch 'O=Microsoft Corporation') {
    Write-Error $subject
    exit 3
}

exit 0
