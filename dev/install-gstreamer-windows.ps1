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

$url = "https://gstreamer.freedesktop.org/data/pkg/windows/$Version/msvc/gstreamer-1.0-msvc-x86_64-$Version.exe"
$exe = Join-Path ([IO.Path]::GetTempPath()) "gstreamer-$Version.exe"
Write-Host "downloading $url"
Invoke-WebRequest -Uri $url -OutFile $exe
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

if ($env:GITHUB_ENV) {
    "PKG_CONFIG_PATH=$gst\lib\pkgconfig" | Out-File -FilePath $env:GITHUB_ENV -Append
    "GSTREAMER_1_0_ROOT_MSVC_X86_64=$gst\" | Out-File -FilePath $env:GITHUB_ENV -Append
    "$gst\bin" | Out-File -FilePath $env:GITHUB_PATH -Append
}
