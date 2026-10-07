# Helpers for login.ps1: installing SimplySign Desktop, finding its window and
# typing into it. Dot sourced, never run on its own.
#
# Certum documents no command line or API login for SimplySign Desktop, only
# the window. So the login is typed into that window with WScript.Shell's
# SendKeys, which works on a GitHub hosted Windows runner because the runner
# has a logged in desktop session. The sequence (start the app twice, the
# second start brings up the login window, user name, Tab, code, Enter, the
# window closes on success) follows jay0lee/certum-cloud-code-sign, whose CI
# drives the same window on windows-2022 and windows-2025:
# https://github.com/jay0lee/certum-cloud-code-sign/blob/a3324503499c49090869856eeeecfe95fb613d87/scripts/auth.mjs

# The 64 bit installer from Certum's download page,
# https://support.certum.eu/en/cert-offer-software-and-libraries/
$SimplySignVersion = '9.4.4.92'
$SimplySignMsi = "https://files.certum.eu/software/SimplySignDesktop/Windows/$SimplySignVersion/SimplySignDesktop-$SimplySignVersion-64-bit-en.msi"
$SimplySignExe = 'C:\Program Files\Certum\SimplySign Desktop\SimplySignDesktop.exe'

function Install-SimplySign {
    if (Test-Path $SimplySignExe) { Write-Host "SimplySign Desktop is already installed"; return }
    $msi = Join-Path $env:RUNNER_TEMP 'SimplySignDesktop.msi'
    $log = Join-Path $env:RUNNER_TEMP 'simplysign-install.log'
    for ($try = 1; $try -le 3; $try++) {
        try { Invoke-WebRequest $SimplySignMsi -OutFile $msi -UseBasicParsing; break }
        catch { if ($try -eq 3) { throw "could not download $SimplySignMsi`: $_" }; Start-Sleep 5 }
    }
    # Certum publishes no checksum, but it does sign the installer.
    $sig = Get-AuthenticodeSignature $msi
    if ($sig.Status -ne 'Valid') { throw "the SimplySign installer's signature is $($sig.Status), so it was not run" }
    Write-Host "SimplySign installer signed by $($sig.SignerCertificate.Subject)"
    $p = Start-Process msiexec.exe -ArgumentList "/i `"$msi`" /qn /norestart /l*v `"$log`"" -Wait -PassThru
    if ($p.ExitCode -notin 0, 3010) {
        Get-Content $log -Tail 40 | Write-Host
        throw "msiexec exited $($p.ExitCode) installing SimplySign Desktop"
    }
    if (-not (Test-Path $SimplySignExe)) { throw "SimplySign Desktop installed, but $SimplySignExe is not there" }
}

function Get-SimplySignWindow {
    Get-Process | Where-Object { $_.ProcessName -like '*SimplySign*' -and $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle } |
        Select-Object -First 1
}

# The first start leaves the app in the tray; the second shows the login window.
function Open-SimplySignLogin {
    for ($try = 1; $try -le 5; $try++) {
        Start-Process $SimplySignExe
        for ($i = 0; $i -lt 10; $i++) {
            Start-Sleep 1
            $w = Get-SimplySignWindow
            if ($w) { Write-Host "login window: '$($w.MainWindowTitle)'"; return $w }
        }
    }
    throw 'the SimplySign Desktop login window never appeared'
}

function Send-ToWindow($Process, [string] $Keys) {
    $shell = New-Object -ComObject WScript.Shell
    [void]$shell.AppActivate($Process.Id)
    Start-Sleep -Milliseconds 300
    $shell.SendKeys($Keys)
}

# SendKeys treats + ^ % ~ ( ) { } [ ] as commands, so each is braced.
function ConvertTo-SendKeysText([string] $Text) {
    return [regex]::Replace($Text, '[+^%~(){}\[\]]', { param($m) '{' + $m.Value + '}' })
}

function Save-Screen([string] $Name) {
    if (-not $env:SIMPLYSIGN_SCREENSHOTS) { return }
    try {
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
        New-Item -ItemType Directory -Force $env:SIMPLYSIGN_SCREENSHOTS | Out-Null
        $bmp.Save((Join-Path $env:SIMPLYSIGN_SCREENSHOTS "$Name.png"))
        $g.Dispose(); $bmp.Dispose()
    } catch { Write-Host "no screenshot: $_" }
}

# The newest code signing certificate whose key SimplySign has made available.
function Find-CodeSigningCert {
    Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date) -and
                       ($_.EnhancedKeyUsageList.ObjectId -contains '1.3.6.1.5.5.7.3.3') } |
        Sort-Object NotAfter -Descending | Select-Object -First 1
}

# The newest x64 signtool in the Windows SDK, which windows-latest carries.
function Find-SignTool {
    $found = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
    if (-not $found) { throw 'no x64 signtool.exe under the Windows 10 SDK; install the Windows SDK signing tools' }
    return $found.FullName
}
