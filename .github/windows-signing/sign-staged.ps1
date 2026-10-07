# Before `cargo tauri build`: signs every .exe and .dll staged for the
# installers that is not signed already, and writes the Tauri config that
# makes Tauri sign the rest through sign.ps1.
#
# Tauri's bundle.windows.signCommand signs the app after it patches it for
# each installer, the sidecar, both installers, the NSIS uninstaller and
# plugins, and every .exe and .dll among the resources that is not signed yet
# (tauri-bundler 2.9: bundle.rs, bundle/windows/sign.rs, msi/mod.rs and
# nsis/mod.rs). The resources are the GStreamer runtime, the plugins and the
# browser renderer, a few hundred files. Tauri would sign them one signtool
# call each; signing them here in batches is quicker, and Tauri then finds
# them signed and leaves them.
#
# Run from the repository root. Writes WINDOWS_SIGN_CONFIG to GITHUB_ENV, for
# `cargo tauri build ... $WINDOWS_SIGN_CONFIG`.
$ErrorActionPreference = 'Stop'
$dirs = 'binaries', 'plugins', 'browser', 'gstreamer', 'onnxruntime' |
    ForEach-Object { "tauri-app/$_" } | Where-Object { Test-Path $_ }
$all = @(Get-ChildItem $dirs -Recurse -File -Include *.exe, *.dll)
$unsigned = @($all | Where-Object { (Get-AuthenticodeSignature $_.FullName).Status -ne 'Valid' } | ForEach-Object FullName)
Write-Host "$($all.Count) executables and libraries staged, $($unsigned.Count) to sign"
if ($unsigned.Count) {
    $t = Measure-Command { & (Join-Path $PSScriptRoot 'sign.ps1') $unsigned }
    Write-Host ("signed in {0:n0} s" -f $t.TotalSeconds)
}

$script = Join-Path $PSScriptRoot 'sign.ps1'
$conf = @{ bundle = @{ windows = @{ signCommand = @{
    cmd = 'pwsh'; args = @('-NoProfile', '-NonInteractive', '-File', $script, '%1') } } } }
$path = Join-Path $env:RUNNER_TEMP 'windows-sign.conf.json'
$conf | ConvertTo-Json -Depth 8 | Set-Content -Path $path -Encoding utf8
Get-Content $path | Write-Host
Add-Content $env:GITHUB_ENV "WINDOWS_SIGN_CONFIG=--config $path"
