# Let this workspace's own builds take connections from the local network
# without Windows asking for each one, and clear out the block rules that
# earlier prompts left behind.
#
# Windows Firewall asks once per program path whenever a program listens on
# every interface, and a cargo build makes a new path for every test binary.
# So does an unpacked installer, a test install under %TEMP%, and every plugin
# copy under %APPDATA%. Left alone, a day of testing is dozens of prompts.
#
# A prompt that is cancelled, or answered for one kind of network only, leaves
# a Block rule for that path. Block wins over Allow in Windows Firewall, so one
# old Block rule silently stops the installer's own rules from working: NDI,
# SRT, RTMP or a phone on the Wi-Fi just never connects. This script removes
# Block rules for GodwinMix programs that Windows made from a prompt (they
# belong to no group) and leaves every other rule alone.
#
# Then it adds one allow rule per program, inbound, limited to the local
# subnet, so a phone on the same Wi-Fi reaches a test mixer and nothing on the
# internet does. Loopback is never filtered, so tests that stay on 127.0.0.1
# need none of this.
#
# Run it from an elevated PowerShell after a build that added test binaries,
# or after unpacking a test build somewhere. It replaces its own rules each
# time.
#
#   pwsh -File dev/windows-firewall.ps1                    # clear blocks, add or refresh
#   pwsh -File dev/windows-firewall.ps1 -Path C:\gm-test   # also allow builds under that folder
#   pwsh -File dev/windows-firewall.ps1 -List              # print what it would do, change nothing
#   pwsh -File dev/windows-firewall.ps1 -Remove            # take its allow rules out

param(
    [switch]$Remove,
    [switch]$List,
    [string[]]$Path = @()
)

$group = "GodwinMix dev builds"
$repo = Split-Path -Parent $PSScriptRoot
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo "target" }
$names = '^(godwinmix|gmx)[-_].*\.exe$|^(godwinmix|gmx)\.exe$'

if ($Remove) {
    Get-NetFirewallRule -Group $group -ErrorAction SilentlyContinue | Remove-NetFirewallRule
    "removed the '$group' rules"
    return
}

# Block rules Windows made from a prompt for one of our programs. Prompt rules
# have no group; rules a person or another tool wrote on purpose usually do,
# and those are left alone.
$blocks = Get-NetFirewallRule -Action Block -ErrorAction SilentlyContinue |
    Where-Object { -not $_.Group } |
    Where-Object {
        $program = ($_ | Get-NetFirewallApplicationFilter).Program
        $program -and (Split-Path -Leaf $program) -match $names
    }

$programs = @(foreach ($profile in "debug", "release") {
    foreach ($dir in (Join-Path $target $profile), (Join-Path $target "$profile\deps")) {
        if (Test-Path $dir) { Get-ChildItem $dir -Filter *.exe | Where-Object Name -match $names | ForEach-Object FullName }
    }
})
# Plugin copies a dev run of the desktop app makes, and any folder named on
# the command line (an unpacked MSI, a test install).
$plugins = Join-Path $env:APPDATA "mix.godwin.desktop\plugins"
$programs += foreach ($dir in @($plugins) + $Path) {
    if (Test-Path $dir) { Get-ChildItem $dir -Recurse -File -Filter *.exe | Where-Object Name -match $names | ForEach-Object FullName }
}
$gst = Get-Command gst-launch-1.0.exe -ErrorAction SilentlyContinue
if ($gst) { $programs += $gst.Source }
$programs = @($programs | Sort-Object -Unique)

if ($List) {
    "would remove $(@($blocks).Count) prompt block rules:"
    $blocks | ForEach-Object { "  $($_.DisplayName) [$($_.Profile)]: $(($_ | Get-NetFirewallApplicationFilter).Program)" }
    "would allow $($programs.Count) programs:"
    $programs | ForEach-Object { "  $_" }
    return
}

$blocks | Remove-NetFirewallRule
Get-NetFirewallRule -Group $group -ErrorAction SilentlyContinue | Remove-NetFirewallRule
foreach ($program in $programs) {
    New-NetFirewallRule -DisplayName "GodwinMix dev: $(Split-Path $program -Leaf)" -Group $group `
        -Direction Inbound -Action Allow -Program $program -RemoteAddress LocalSubnet `
        -Profile Private, Public | Out-Null
}
"removed $(@($blocks).Count) prompt block rules, allowed $($programs.Count) programs from the local subnet under '$group'"
