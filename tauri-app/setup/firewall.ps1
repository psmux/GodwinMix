# Windows Firewall rules for GodwinMix, written by both Windows installers.
#
# Windows asks "allow this app?" the first time a program listens on the
# network, once for each exact path. The mixer asked, every plugin that can
# listen asked (RTMP, SRT, RTSP, WHIP, NDI, UDP and the rest), and they all
# asked again after an update, because the app runs its plugins from copies
# under %APPDATA%\mix.godwin.desktop\plugins\<name>\<version>. Rules for those
# exact paths, made while the installer already holds admin rights, mean
# nobody is asked.
#
# Private and domain networks only. On a network Windows calls public the
# question still comes, which is the right moment for it.
#
# Every rule is in one group, so an install removes the group and writes it
# again and the uninstaller removes it. A rule for a program that never
# listens opens nothing. The script never fails an install: an error goes to
# %TEMP%\godwinmix-firewall.log and it exits 0.
#
#   firewall.ps1 -Action Install -InstallDir <dir> -AppData <roaming AppData>
#   firewall.ps1 -Action Uninstall
#   firewall.ps1 -Action List -InstallDir <dir> -AppData <dir>   (prints, changes nothing)

param(
    [ValidateSet('Install', 'Uninstall', 'List')] [string] $Action = 'List',
    [string] $InstallDir = (Split-Path -Parent $PSScriptRoot),
    [string] $AppData = $env:APPDATA,
    [string] $Group = 'GodwinMix'
)

# The app's bundle identifier, which names its folder in AppData.
$BundleId = 'mix.godwin.desktop'

# A path as an installer passes it, with any trailing "\." or "\" taken off.
# The MSI adds the "." because "C:\dir\" before a closing quote would escape it.
function Get-CleanPath([string] $Path) {
    return [IO.Path]::GetFullPath($Path).TrimEnd('\')
}

# The version in a manifest's [plugin] table, read the way the app reads it.
function Get-PluginVersion([string] $Dir) {
    $manifest = Join-Path $Dir 'gmx-plugin.toml'
    if (-not (Test-Path -LiteralPath $manifest)) { return $null }
    $inPlugin = $false
    foreach ($line in Get-Content -LiteralPath $manifest) {
        $t = $line.Trim()
        if ($t.StartsWith('[')) { $inPlugin = $t -eq '[plugin]'; continue }
        if ($inPlugin -and $t -match '^version\s*=\s*"?([^"]+)"?') { return $Matches[1].Trim() }
    }
    return $null
}

# Where each executable in a bundled plugin runs from once the app has copied
# it: <AppData>\mix.godwin.desktop\plugins\<name>\<version>\<same relative path>.
function Get-CopiedExes([string] $From, [string] $Name, [string] $Version, [string] $Seeded) {
    foreach ($exe in Get-ChildItem -LiteralPath $From -Recurse -File -Filter *.exe) {
        $rel = $exe.FullName.Substring($From.Length).TrimStart('\')
        Join-Path $Seeded "$Name\$Version\$rel"
    }
}

# Every program GodwinMix runs that might listen on the network.
function Get-GmxPrograms([string] $InstallDir, [string] $AppData) {
    $InstallDir = Get-CleanPath $InstallDir
    $AppData = Get-CleanPath $AppData
    $seeded = Join-Path $AppData "$BundleId\plugins"
    $plugins = Join-Path $InstallDir 'plugins'
    $found = @(Join-Path $InstallDir 'godwinmix.exe')
    if (Test-Path -LiteralPath $plugins) {
        # Device plugins, staged as plugins\windows\<name>\<version> and
        # copied at every launch (tauri-app/src/plugins.rs).
        $device = Join-Path $plugins 'windows'
        if (Test-Path -LiteralPath $device) {
            foreach ($name in Get-ChildItem -LiteralPath $device -Directory) {
                foreach ($ver in Get-ChildItem -LiteralPath $name.FullName -Directory) {
                    if (-not (Test-Path -LiteralPath (Join-Path $ver.FullName 'gmx-plugin.toml'))) { continue }
                    $found += Get-CopiedExes $ver.FullName $name.Name $ver.Name $seeded
                }
            }
        }
        # Every other plugin, staged as plugins\<name> and installed by the
        # mixer the first time its feature is picked (tauri-app/src/shipped.rs).
        foreach ($dir in Get-ChildItem -LiteralPath $plugins -Directory) {
            if ($dir.Name -eq 'windows') { continue }
            $version = Get-PluginVersion $dir.FullName
            if ($version) { $found += Get-CopiedExes $dir.FullName $dir.Name $version $seeded }
        }
    }
    return $found | Select-Object -Unique
}

function Write-Log([string] $Message) {
    try { Add-Content -LiteralPath (Join-Path $env:TEMP 'godwinmix-firewall.log') -Value "$(Get-Date -Format s) $Message" } catch { }
}

if ($Action -eq 'List') {
    Get-GmxPrograms $InstallDir $AppData
    exit 0
}

try {
    Get-NetFirewallRule -Group $Group -ErrorAction SilentlyContinue | Remove-NetFirewallRule
    if ($Action -eq 'Uninstall') {
        Write-Log 'removed the GodwinMix firewall rules'
        exit 0
    }
    $programs = @(Get-GmxPrograms $InstallDir $AppData)
    foreach ($program in $programs) {
        $rule = @{
            DisplayName = "GodwinMix: $(Split-Path -Leaf $program)"
            Group       = $Group
            Description = "Lets $program accept connections on private networks without Windows asking. Written by the GodwinMix installer and removed by its uninstaller."
            Direction   = 'Inbound'
            Action      = 'Allow'
            Program     = $program
            Profile     = 'Domain, Private'
        }
        New-NetFirewallRule @rule | Out-Null
    }
    Write-Log "wrote $($programs.Count) GodwinMix firewall rules for $InstallDir and $AppData"
} catch {
    Write-Log "the firewall rules were not written: $_"
}
exit 0
