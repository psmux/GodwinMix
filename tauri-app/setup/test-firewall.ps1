# Checks the paths firewall.ps1 writes rules for, against a made up install
# and AppData. Needs no admin rights and changes no rule: it runs List only.
#
#   pwsh tauri-app/setup/test-firewall.ps1

$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ("gmx-firewall-" + [Guid]::NewGuid().ToString('N'))
$install = Join-Path $root 'Program Files\GodwinMix'
$appdata = Join-Path $root 'Roaming'

function Touch([string] $Path, [string] $Text = '') {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    Set-Content -LiteralPath $Path -Value $Text
}

try {
    Touch "$install\godwinmix.exe"
    # A device plugin, staged per platform with its version in the path.
    Touch "$install\plugins\windows\ingest\0.1.0\gmx-plugin.toml" "[plugin]`nname = `"ingest`"`nversion = `"0.1.0`""
    Touch "$install\plugins\windows\ingest\0.1.0\bin\gmx-ingest.exe"
    # A plugin installed on first use, flat, its version only in the manifest
    # and a version key in another table that must not be read.
    Touch "$install\plugins\srt\gmx-plugin.toml" "[engine]`nversion = `"9.9.9`"`n[plugin]`nname = `"srt`"`nversion = `"0.3.1`""
    Touch "$install\plugins\srt\bin\gmx-srt.exe"
    # A folder with no manifest is not a plugin.
    Touch "$install\plugins\stray\bin\stray.exe"

    $got = @(& (Join-Path $PSScriptRoot 'firewall.ps1') -Action List -InstallDir "$install\." -AppData "$appdata\")
    $want = @(
        "$install\godwinmix.exe",
        "$appdata\mix.godwin.desktop\plugins\ingest\0.1.0\bin\gmx-ingest.exe",
        "$appdata\mix.godwin.desktop\plugins\srt\0.3.1\bin\gmx-srt.exe"
    )
    $missing = $want | Where-Object { $_ -notin $got }
    $extra = $got | Where-Object { $_ -notin $want }
    if ($missing -or $extra) {
        throw "firewall paths differ.`nmissing: $($missing -join ', ')`nextra: $($extra -join ', ')"
    }
    Write-Host "ok: $($got.Count) firewall paths, the mixer and each plugin where the app runs it"
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
