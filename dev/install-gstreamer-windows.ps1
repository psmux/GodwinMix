# Installs the official GStreamer for Windows (MSVC, full type) on a CI
# runner and tells the later steps where it is.
#
# Not chocolatey. Its package is 1.26 and runs the MSI with the typical
# feature set, which leaves out the SRT plugin among others: srtsrc was
# missing, a parse of `srtsrc uri=...` fell back to an element with no such
# property, and every SRT test failed on Windows while passing on a desk with
# the full install. CONTRIBUTING.md tells a person to install exactly this.
#
#   ./dev/install-gstreamer-windows.ps1 [-Version 1.28.6]

param([string]$Version = "1.28.6")

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$name = "gstreamer-1.0-msvc-x86_64-$Version.exe"
$path = "data/pkg/windows/$Version/msvc/$name"
$exe = Join-Path ([IO.Path]::GetTempPath()) "gstreamer-$Version.exe"

# Three places have the same file, tried in this order:
#
# 1. A release asset of psmux/godwinmix-ci-deps, served by GitHub. Through
#    the whole of the 0.3.11 release freedesktop.org cut this half gigabyte
#    short and answered 503 for hours, and failed the Windows installer build
#    four times, so the unmodified file is kept there too. A new GStreamer
#    version is put there by that repository's "mirror GStreamer for Windows"
#    workflow before this script's default version moves to it.
# 2. origin.gstreamer.freedesktop.org, the server behind freedesktop.org's
#    CDN. The 503s came from the CDN's cache, not from the server.
# 3. gstreamer.freedesktop.org, the CDN itself.
#
# Every copy has to match the SHA256 known for its version.
$known = @{
    "1.28.6" = "SHA256_1_28_6"
}
$sources = @(
    "https://github.com/psmux/godwinmix-ci-deps/releases/download/gstreamer-$Version/$name",
    "https://origin.gstreamer.freedesktop.org/$path",
    "https://gstreamer.freedesktop.org/$path"
)

function Test-Installer {
    if (-not (Test-Path $exe)) { return $false }
    if (-not $known.ContainsKey($Version)) {
        Write-Host "::warning::no SHA256 is known for GStreamer $Version; add it to this script"
        return $true
    }
    return (Get-FileHash -Algorithm SHA256 $exe).Hash -eq $known[$Version].ToUpper()
}

# One source. A server that ends a response part way is carried on from where
# the last try stopped, so every try adds to the file, and the source is given
# up when ten tries in a row have added nothing.
function Get-From([string]$url) {
    $size = $null
    for ($try = 1; $try -le 5 -and -not $size; $try++) {
        try {
            $size = [int64](Invoke-WebRequest -Uri $url -Method Head -TimeoutSec 60).Headers["Content-Length"][0]
        } catch {
            Write-Host "::warning::asking the size of $url failed: $($_.Exception.Message)"
            Start-Sleep -Seconds (5 * $try)
        }
    }
    if (-not $size) { return $false }
    Remove-Item $exe -ErrorAction SilentlyContinue
    $have = 0
    $stuck = 0
    for ($try = 1; $have -lt $size; $try++) {
        Write-Host "downloading $url (try $try, $have of $size bytes)"
        curl.exe --location --fail --silent --show-error --continue-at - --speed-limit 100000 --speed-time 60 --output $exe $url
        $now = if (Test-Path $exe) { (Get-Item $exe).Length } else { 0 }
        $stuck = if ($now -gt $have) { 0 } else { $stuck + 1 }
        $have = $now
        if ($stuck -ge 10) {
            Write-Host "::warning::$url stopped at $have of $size bytes"
            return $false
        }
        if ($stuck -gt 0) { Start-Sleep -Seconds ([math]::Min(60, 5 * $stuck)) }
    }
    return $true
}

$got = $false
foreach ($url in $sources) {
    if ((Get-From $url) -and (Test-Installer)) {
        Write-Host "the copy from $url matches its SHA256"
        $got = $true
        break
    }
    Write-Host "::warning::no good copy of $name from $url; trying the next place"
}
if (-not $got) { throw "no source gave a GStreamer $Version installer matching its SHA256: $($sources -join ', ')" }
$p = Start-Process -FilePath $exe -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/TYPE=full" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "the GStreamer installer exited with $($p.ExitCode)" }

$gst = "C:\Program Files\gstreamer\1.0\msvc_x86_64"
if (-not (Test-Path "$gst\bin\gst-inspect-1.0.exe")) {
    throw "GStreamer $Version did not land in $gst; look for it under Program Files and change this script"
}
& "$gst\bin\gst-inspect-1.0.exe" --version
foreach ($element in "srtsrc", "x265enc", "cmafmux") {
    & "$gst\bin\gst-inspect-1.0.exe" --exists $element
    if ($LASTEXITCODE -ne 0) { Write-Host "::warning::$element is not in this GStreamer" }
}

# The first SVG with text on a fresh machine makes fontconfig scan every
# font and write its cache, which took longer than the ten seconds a picture
# decode is given, and three graphics tests that started together all failed
# at once. One render here pays for it before the tests start.
$svg = Join-Path ([IO.Path]::GetTempPath()) "gmx-warm.svg"
'<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><text x="2" y="20">Warm</text></svg>' | Set-Content -Encoding ascii $svg
$warm = Measure-Command { & "$gst\bin\gst-launch-1.0.exe" -q filesrc location="$($svg -replace '\\', '/')" ! rsvgdec ! fakesink }
Write-Host ("fontconfig warmed in {0:N1} s" -f $warm.TotalSeconds)

if ($env:GITHUB_ENV) {
    "PKG_CONFIG_PATH=$gst\lib\pkgconfig" | Out-File -FilePath $env:GITHUB_ENV -Append
    "GSTREAMER_1_0_ROOT_MSVC_X86_64=$gst\" | Out-File -FilePath $env:GITHUB_ENV -Append
    "$gst\bin" | Out-File -FilePath $env:GITHUB_PATH -Append
}
