# After `cargo tauri build`: both installers must exist, and verify.ps1 opens
# each and checks everything in it. The app in target/release is no use for
# this, since Tauri puts the unsigned copy back there after bundling.
$ErrorActionPreference = 'Stop'
$bundle = 'tauri-app/target/release/bundle'
$nsis = @(Get-ChildItem "$bundle/nsis/*-setup.exe" -ErrorAction SilentlyContinue)
$msi = @(Get-ChildItem "$bundle/msi/*.msi" -ErrorAction SilentlyContinue)
if (-not $nsis -or -not $msi) {
    throw "expected a -setup.exe and an .msi under $bundle, found $($nsis.Count) and $($msi.Count)"
}
& (Join-Path $PSScriptRoot 'verify.ps1') ($nsis + $msi).FullName
exit $LASTEXITCODE
