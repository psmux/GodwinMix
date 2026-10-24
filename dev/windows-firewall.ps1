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
# or after unpacking a test build somewhere. It keeps its own rules for
# programs that still exist and drops the rest.
#
#   pwsh -File dev/windows-firewall.ps1                    # clear blocks, add or refresh
#   pwsh -File dev/windows-firewall.ps1 -Path C:\gm-test   # also allow builds under that folder
#   pwsh -File dev/windows-firewall.ps1 -List              # print what it would do, change nothing
#   pwsh -File dev/windows-firewall.ps1 -Remove            # take its allow rules out
#
# It finds the builds in the checkout it sits in. A copy saved anywhere else
# needs -Repo <checkout> (or CARGO_TARGET_DIR). It also allows gst-launch-1.0
# when it is on PATH, since the tests run GStreamer pipelines that listen.

param(
    [switch]$Remove,
    [switch]$List,
    [string[]]$Path = @(),
    [string]$Repo = (Split-Path -Parent $PSScriptRoot)
)

$group = "GodwinMix dev builds"
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Repo "target" }
$names = '^(godwinmix|gmx)[-_].*\.exe$|^(godwinmix|gmx)\.exe$'

if ($Remove) {
    Get-NetFirewallRule -Group $group -ErrorAction SilentlyContinue | Remove-NetFirewallRule
    "removed the '$group' rules"
    return
}

# Block rules Windows made from a prompt for one of our programs. Prompt rules
# have no group; rules a person or another tool wrote on purpose usually do,
# and those are left alone.
# One call for every rule's program, joined on the rule's id; asking rule by
# rule takes minutes on a machine with a few thousand rules.
$programOf = @{}
foreach ($filter in Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue) {
    $programOf[$filter.InstanceID] = $filter.Program
}
$blocks = @(Get-NetFirewallRule -Action Block -ErrorAction SilentlyContinue |
    Where-Object { -not $_.Group } |
    Where-Object {
        $program = $programOf[$_.InstanceID]
        $program -and (Split-Path -Leaf $program) -match $names
    })

$programs = @(foreach ($build in "debug", "release") {
    foreach ($dir in (Join-Path $target $build), (Join-Path $target "$build\deps")) {
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
    $blocks | ForEach-Object { "  $($_.DisplayName) [$($_.Profile)]: $($programOf[$_.InstanceID])" }
    "would allow $($programs.Count) programs:"
    $programs | ForEach-Object { "  $_" }
    return
}

$blocks | Remove-NetFirewallRule
# Keep this script's rules for programs that still exist, so a run from
# another folder or another checkout does not drop them; rules for deleted
# builds go.
$kept = @{}
foreach ($rule in Get-NetFirewallRule -Group $group -ErrorAction SilentlyContinue) {
    $program = $programOf[$rule.InstanceID]
    if ($program -and (Test-Path -LiteralPath $program) -and -not $kept[$program]) { $kept[$program] = $true }
    else { $rule | Remove-NetFirewallRule }
}
foreach ($program in $programs | Where-Object { -not $kept[$_] }) {
    New-NetFirewallRule -DisplayName "GodwinMix dev: $(Split-Path $program -Leaf)" -Group $group `
        -Direction Inbound -Action Allow -Program $program -RemoteAddress LocalSubnet `
        -Profile Private, Public | Out-Null
}
"removed $(@($blocks).Count) prompt block rules, kept $($kept.Count) and allowed $(@($programs | Where-Object { -not $kept[$_] }).Count) more programs from the local subnet under '$group'"
