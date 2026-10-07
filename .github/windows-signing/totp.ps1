# A time based one time password (RFC 6238) from a Base32 secret, which is
# what SimplySign's mobile app shows and what its login window asks for.
# Dot source this file, then call Get-Totp. The standard library has the HMAC,
# so nothing is installed for it.
#
# Certum's SimplySign secrets use SHA-256 with six digits and a thirty second
# step. The algorithm is taken from the CERTUM_TOTP_ALGORITHM variable rather
# than assumed, and SHA1, SHA-1, sha256 and SHA-256 are all accepted.
#
# Checked against the RFC 6238 appendix B vectors by test-totp.ps1.

function ConvertFrom-Base32([string] $Text) {
    $alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
    $clean = ($Text -replace '[\s=-]', '').ToUpperInvariant()
    if (-not $clean) { throw 'the TOTP secret is empty' }
    $bytes = [System.Collections.Generic.List[byte]]::new()
    $buffer = 0; $bits = 0
    foreach ($ch in $clean.ToCharArray()) {
        $value = $alphabet.IndexOf($ch)
        if ($value -lt 0) { throw 'the TOTP secret is not Base32: check CERTUM_TOTP_SECRET' }
        $buffer = (($buffer -shl 5) -bor $value) -band 0xFFFF
        $bits += 5
        if ($bits -ge 8) {
            $bits -= 8
            $bytes.Add([byte](($buffer -shr $bits) -band 0xFF))
        }
    }
    return , $bytes.ToArray()
}

function New-TotpHmac([string] $Algorithm, [byte[]] $Key) {
    switch (($Algorithm -replace '[\s_-]', '').ToUpperInvariant()) {
        'SHA1' { return [System.Security.Cryptography.HMACSHA1]::new($Key) }
        'SHA256' { return [System.Security.Cryptography.HMACSHA256]::new($Key) }
        'SHA512' { return [System.Security.Cryptography.HMACSHA512]::new($Key) }
        default { throw "CERTUM_TOTP_ALGORITHM is '$Algorithm'; set it to SHA-256, SHA-1 or SHA-512" }
    }
}

function Get-Totp {
    param(
        [Parameter(Mandatory)] [string] $Secret,
        [string] $Algorithm = 'SHA-256',
        [int] $Digits = 6,
        [int] $Period = 30,
        [long] $UnixTime = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds(),
        [byte[]] $Key
    )
    if (-not $Key) { $Key = ConvertFrom-Base32 $Secret }
    $counter = [BitConverter]::GetBytes([long][Math]::Floor($UnixTime / $Period))
    if ([BitConverter]::IsLittleEndian) { [Array]::Reverse($counter) }
    $hmac = New-TotpHmac $Algorithm $Key
    try { $hash = $hmac.ComputeHash($counter) } finally { $hmac.Dispose() }
    $offset = $hash[$hash.Length - 1] -band 0x0F
    $binary = (([int]($hash[$offset] -band 0x7F)) -shl 24) -bor
              (([int]$hash[$offset + 1]) -shl 16) -bor
              (([int]$hash[$offset + 2]) -shl 8) -bor
              ([int]$hash[$offset + 3])
    $code = $binary % [int][Math]::Pow(10, $Digits)
    return $code.ToString().PadLeft($Digits, '0')
}

# Seconds left before the current code stops being accepted.
function Get-TotpSecondsLeft([int] $Period = 30) {
    return $Period - ([DateTimeOffset]::UtcNow.ToUnixTimeSeconds() % $Period)
}
