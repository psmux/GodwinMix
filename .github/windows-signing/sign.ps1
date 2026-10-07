# Signs the files named on the command line with the certificate login.ps1
# found, and timestamps each signature at Certum's RFC 3161 server so it stays
# valid after the certificate expires. The flags are the ones in Certum's own
# signtool guide:
# https://www.files.certum.eu/documents/manual_en/Signing_with_the_use_of_jarsigner_tool_and_signtool.pdf
#
# Used twice in the release: on its own for the files staged before
# `cargo tauri build`, and as Tauri's bundle.windows.signCommand, which calls
# it once per file with the path in place of %1.
#
# signtool takes many files in one call, and each call opens one smart card
# session, so files go in batches. A batch that fails is tried again twice,
# because the timestamp server refusing a request now and then is the usual
# reason, and signing a file again replaces its signature rather than adding
# a second one.
param([Parameter(Mandatory, ValueFromRemainingArguments)] [string[]] $Files)
$ErrorActionPreference = 'Stop'

if (-not $env:SIGNTOOL -or -not $env:SIGN_THUMBPRINT) {
    throw 'SIGNTOOL and SIGN_THUMBPRINT are not set; run login.ps1 first'
}
$common = @('sign', '/sha1', $env:SIGN_THUMBPRINT, '/fd', 'sha256',
            '/tr', 'http://time.certum.pl', '/td', 'sha256', '/d', 'GodwinMix')
$batch = 25
for ($start = 0; $start -lt $Files.Count; $start += $batch) {
    $chunk = @($Files[$start..([Math]::Min($start + $batch, $Files.Count) - 1)])
    for ($try = 1; ; $try++) {
        & $env:SIGNTOOL @common @chunk
        if ($LASTEXITCODE -eq 0) { break }
        if ($try -ge 3) { throw "signtool exited $LASTEXITCODE signing $($chunk -join ', ')" }
        Write-Host "signtool exited $LASTEXITCODE, trying again in $(10 * $try) s"
        Start-Sleep (10 * $try)
    }
}
