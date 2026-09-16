<#
.SYNOPSIS
    Put a trimmed GStreamer inside the desktop app, on Windows.

.DESCRIPTION
    Windows is the one platform where the media stack cannot be left to the
    machine. GStreamer's own runtime installer is 527 MB, it is a separate
    download, and asking somebody with four hours before a service to install
    it first is the same as telling them to use something else. The budget for
    the whole GodwinMix installer is 150 MB.

    This downloads the official MSVC runtime MSI (not the development one,
    which is the larger half of the 527 MB, and not MinGW), extracts it
    without installing anything, and hands it to dev/gst_trim.py. The trimmer
    is the same one dev/bundle-gstreamer.sh calls on macOS and Linux, so the
    keep list follows codecs.toml on every platform and only the fetching
    differs.

    The result goes in tauri-app\gstreamer\windows\, which tauri.conf.json
    lists under bundle.resources and which the shell finds at
    resource_dir()\gstreamer.

.PARAMETER From
    A GStreamer prefix already on this machine, for example
    "C:\gstreamer\1.0\msvc_x86_64". Skips the download.

.PARAMETER Version
    Which release to download. Defaults to the one the project is tested on.

.PARAMETER Out
    Where to write the trimmed tree.

.PARAMETER BudgetMb
    Refuse to finish if the tree is bigger than this. The default is 130,
    which is what the 150 MB installer budget leaves once the shell and the
    mixer have had their 17 MB.

.PARAMETER ExcludeGpl
    Leave out plugins whose licence is GPL (x264, x265). The catalogue falls
    back to openh264, which is why it carries a licence field.

.EXAMPLE
    dev\bundle-gstreamer.ps1
    dev\bundle-gstreamer.ps1 -From C:\gstreamer\1.0\msvc_x86_64
    dev\bundle-gstreamer.ps1 -Version 1.28.7 -BudgetMb 130
#>
[CmdletBinding()]
param(
    [string]$From = "",
    [string]$Version = "1.28.7",
    [string]$Out = "",
    [double]$BudgetMb = 130,
    [switch]$ExcludeGpl
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
if (-not $Out) { $Out = Join-Path $repo "tauri-app\gstreamer\windows" }

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x86_64" }

function Find-Python {
    foreach ($name in @("python3", "python", "py")) {
        $found = Get-Command $name -ErrorAction SilentlyContinue
        if ($found) { return $found.Source }
    }
    throw "no Python on PATH; dev/gst_trim.py needs one (3.9 or newer)"
}

# --- where the runtime comes from -------------------------------------------

function Get-Runtime {
    param([string]$Release, [string]$Arch)

    $work = Join-Path ([System.IO.Path]::GetTempPath()) ("gmx-gst-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    $msi = Join-Path $work "gstreamer-runtime.msi"
    $url = "https://gstreamer.freedesktop.org/data/pkg/windows/$Release/msvc/gstreamer-1.0-msvc-$Arch-$Release.msi"

    Write-Host "fetching $url"
    # The progress bar makes Invoke-WebRequest an order of magnitude slower on
    # a 200 MB file, which is worth knowing and not worth paying.
    $previous = $ProgressPreference
    $ProgressPreference = "SilentlyContinue"
    try { Invoke-WebRequest -Uri $url -OutFile $msi -UseBasicParsing }
    finally { $ProgressPreference = $previous }

    # An administrative install, which unpacks the MSI into a directory and
    # writes nothing to the registry and nothing to Program Files. No elevation
    # is needed and the machine is left as it was found.
    $target = Join-Path $work "extracted"
    Write-Host "extracting"
    $proc = Start-Process msiexec.exe -Wait -PassThru -ArgumentList @(
        "/a", "`"$msi`"", "/qn", "TARGETDIR=`"$target`""
    )
    if ($proc.ExitCode -ne 0) {
        throw "msiexec failed with $($proc.ExitCode) unpacking $msi"
    }

    $prefix = Get-ChildItem -Path $target -Recurse -Directory -Filter "msvc_*" |
        Select-Object -First 1
    if (-not $prefix) {
        throw "no msvc_* prefix inside the MSI; did the layout change?"
    }
    return $prefix.FullName
}

if (-not $From) {
    $installed = "C:\gstreamer\1.0\msvc_$arch"
    $programFiles = "C:\Program Files\gstreamer\1.0\msvc_$arch"
    if ($env:GSTREAMER_1_0_ROOT_MSVC_X86_64 -and (Test-Path $env:GSTREAMER_1_0_ROOT_MSVC_X86_64)) {
        $From = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64
    } elseif (Test-Path $installed) {
        $From = $installed
    } elseif (Test-Path $programFiles) {
        $From = $programFiles
    } else {
        $From = Get-Runtime -Release $Version -Arch $arch
    }
}

if (-not (Test-Path $From)) {
    throw "no GStreamer to trim at $From"
}

Write-Host "source:   $From"
Write-Host "platform: windows"
Write-Host "out:      $Out"
Write-Host ""

$python = Find-Python
$trimArgs = @(
    (Join-Path $repo "dev\gst_trim.py"),
    "--from", $From,
    "--out", $Out,
    "--platform", "windows",
    "--codecs", (Join-Path $repo "codecs.toml"),
    "--budget-mb", $BudgetMb
)
if ($ExcludeGpl) { $trimArgs += "--exclude-gpl" }

& $python @trimArgs
if ($LASTEXITCODE -ne 0) {
    throw "the trimmed tree is over the $BudgetMb MB budget, or could not be built"
}

# --- prove it loads ---------------------------------------------------------
#
# A tree that is the right size and does not load is worse than no tree at
# all, because the failure only shows up on somebody else's machine. The same
# elements --headless-check asks for are asked for here, out of the bundled
# registry, with the GStreamer installed on this machine shut out.

$plugins = Join-Path $Out "lib\gstreamer-1.0"
$inspect = Join-Path $Out "bin\gst-inspect-1.0.exe"
$registry = Join-Path $Out "..\registry-check.bin"

if (-not (Test-Path $inspect)) { throw "no gst-inspect-1.0.exe in the trimmed tree" }

$env:GST_PLUGIN_PATH = $plugins
$env:GST_PLUGIN_SYSTEM_PATH = $plugins
$env:GST_PLUGIN_SCANNER = Join-Path $Out "libexec\gstreamer-1.0\gst-plugin-scanner.exe"
$env:GST_REGISTRY = $registry
# The DLLs travel in bin\ and Windows finds them on PATH, which is exactly
# what the shell does before it starts the mixer.
$env:PATH = (Join-Path $Out "bin") + ";" + $env:PATH
Remove-Item $registry -ErrorAction SilentlyContinue

Write-Host ""
$failed = 0
foreach ($element in @("compositor", "videoflip", "videocrop", "videoscale", "audiomixer", "proxysink", "rtmp2sink", "srtsink")) {
    Write-Host -NoNewline ("{0,-26}" -f $element)
    & $inspect $element > $null 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Host "ok" }
    else { Write-Host "FAIL (not loadable out of the bundled tree)"; $failed++ }
}

# The catalogue's software H.264 entries, either of which is enough: openh264
# is the licence safe one and x264 is the one most runtimes ship.
Write-Host -NoNewline ("{0,-26}" -f "a software H.264 encoder")
$software = ""
foreach ($element in @("openh264enc", "x264enc")) {
    & $inspect $element > $null 2>&1
    if ($LASTEXITCODE -eq 0) { $software = $element; break }
}
if ($software) { Write-Host "ok ($software)" }
else { Write-Host "FAIL (neither openh264enc nor x264enc is loadable)"; $failed++ }

# Media Foundation is the reason to be on Windows at all, so its absence is
# said out loud without failing a machine that has no hardware encoder.
& $inspect "mfh264enc" > $null 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "note: mfh264enc is not in this tree, so Windows hardware encode is not available"
}

Remove-Item $registry -ErrorAction SilentlyContinue

if ($failed -ne 0) {
    Write-Host ""
    throw "the trimmed tree is missing something the mixer needs"
}
Write-Host ""
Write-Host "OK $Out"
