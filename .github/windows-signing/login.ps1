# Logs a GitHub hosted Windows runner in to Certum's cloud code signing.
#
# SimplySign keeps the private key in Certum's cloud. SimplySign Desktop, once
# logged in with the account name and a one time code, shows the certificate
# to Windows as a virtual smart card, and from then on signtool signs with it
# by thumbprint like any other certificate in CurrentUser\My:
# https://www.files.certum.eu/documents/manual_en/Signing_with_the_use_of_jarsigner_tool_and_signtool.pdf
#
# Reads CERTUM_USERNAME, CERTUM_TOTP_SECRET and CERTUM_TOTP_ALGORITHM from the
# environment (the windows-signing environment's secrets and variable). On
# success it writes SIGNTOOL and SIGN_THUMBPRINT to GITHUB_ENV for sign.ps1,
# and prints the certificate's subject, issuer and expiry. It never prints the
# secret or a code, and masks both, along with the account name.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'totp.ps1')
. (Join-Path $PSScriptRoot 'simplysign.ps1')
. (Join-Path $PSScriptRoot 'window.ps1')

$user = $env:CERTUM_USERNAME
$secret = $env:CERTUM_TOTP_SECRET
$algorithm = if ($env:CERTUM_TOTP_ALGORITHM) { $env:CERTUM_TOTP_ALGORITHM } else { 'SHA-256' }
if (-not $user -or -not $secret) {
    throw 'CERTUM_USERNAME or CERTUM_TOTP_SECRET is empty. They are secrets of the windows-signing environment, which only a v* tag run can use.'
}
Write-Host "::add-mask::$user"
Write-Host "::add-mask::$secret"
$key = ConvertFrom-Base32 $secret

Install-SimplySign
$signtool = Find-SignTool
Write-Host "signtool: $signtool"

# Three tries, each with a fresh code. A code is refused if it was already used,
# which happens when two release jobs log in inside the same thirty seconds,
# so a retry always waits for the next step.
$cert = $null
for ($try = 1; $try -le 3 -and -not $cert; $try++) {
    $window = Open-SimplySignLogin
    Save-Screen "try$try-1-window"
    if ($try -eq 1) { Write-Host 'the login window holds:'; Write-UiTree $window }
    # At least fifteen seconds of validity left when the code is typed.
    $left = Get-TotpSecondsLeft
    if ($try -gt 1 -or $left -lt 15) { Start-Sleep ($left + 1) }
    $code = Get-Totp -Secret 'unused' -Key $key -Algorithm $algorithm
    Write-Host "::add-mask::$code"

    Set-LoginFields $window $user $code
    Write-Host "$($code.Length) digit code, $(Get-TotpSecondsLeft) s left of its step"
    Save-Screen "try$try-2-filled"
    Send-ToWindow $window '{ENTER}'
    Write-Host "login $try submitted, waiting for the certificate"

    # The window closes on success, and the certificate shows up in the store
    # a few seconds later. On failure the window stays with an error on it.
    for ($i = 0; $i -lt 30 -and -not $cert; $i++) {
        Start-Sleep 2
        $cert = Find-CodeSigningCert
    }
    if (-not $cert) {
        Save-Screen "try$try-3-failed"
        $still = Get-SimplySignWindow
        if ($still) {
            $said = (Get-SimplySignMessages | Select-Object -Unique) -join ' | '
            Write-Host "login $try did not take; SimplySign shows: $said"
            Send-ToWindow $still '{ENTER}'
            Start-Sleep 1
            Send-ToWindow $still '{ESC}'
        } else {
            Write-Host "login $try closed the window, but no code signing certificate reached CurrentUser\My"
        }
    }
}
if (-not $cert) {
    Get-ChildItem Cert:\CurrentUser\My | ForEach-Object { Write-Host "in the store: $($_.Subject) (key: $($_.HasPrivateKey))" }
    throw 'SimplySign Desktop did not log in after three codes. Check CERTUM_USERNAME, CERTUM_TOTP_SECRET and that CERTUM_TOTP_ALGORITHM matches the token (Certum uses SHA-256).'
}

Write-Host "certificate: $($cert.Subject)"
Write-Host "issuer:      $($cert.Issuer)"
Write-Host "thumbprint:  $($cert.Thumbprint)"
Write-Host "valid until: $($cert.NotAfter.ToString('yyyy-MM-dd'))"
Add-Content $env:GITHUB_ENV "SIGNTOOL=$signtool"
Add-Content $env:GITHUB_ENV "SIGN_THUMBPRINT=$($cert.Thumbprint)"
