# The RFC 6238 appendix B test vectors, eight digits, for all three hashes,
# plus one Base32 round trip. Run with `pwsh .github/windows-signing/test-totp.ps1`.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'totp.ps1')

$ascii = [System.Text.Encoding]::ASCII
$keys = @{
    'SHA1'    = $ascii.GetBytes('12345678901234567890')
    'SHA-256' = $ascii.GetBytes('12345678901234567890123456789012')
    'SHA512'  = $ascii.GetBytes('1234567890123456789012345678901234567890123456789012345678901234')
}
$vectors = @(
    @(59, '94287082', '46119246', '90693936'),
    @(1111111109, '07081804', '68084774', '25091201'),
    @(1234567890, '89005924', '91819424', '93441116'),
    @(20000000000, '65353130', '77737706', '47863826')
)
$failed = 0
foreach ($v in $vectors) {
    $i = 1
    foreach ($alg in 'SHA1', 'SHA-256', 'SHA512') {
        $got = Get-Totp -Secret 'unused' -Key $keys[$alg] -Algorithm $alg -Digits 8 -UnixTime $v[0]
        if ($got -ne $v[$i]) { Write-Host "FAIL $alg at $($v[0]): got $got, want $($v[$i])"; $failed++ }
        $i++
    }
}
# 'GEZDGNBVGY3TQOJQ' is the Base32 of '1234567890'.
$decoded = $ascii.GetString((ConvertFrom-Base32 'gezd gnbv gy3t qojq'))
if ($decoded -ne '1234567890') { Write-Host "FAIL Base32: got $decoded"; $failed++ }
if ($failed) { exit 1 }
Write-Host 'TOTP: all RFC 6238 vectors pass'
