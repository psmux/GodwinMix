# Let this workspace's own builds take connections from the local network
# without Windows asking for each one.
#
# Windows Firewall asks once per program path whenever a program listens on
# every interface, and a cargo build makes a new path for every test binary.
# Left alone, a day of testing is dozens of prompts. This adds one allow rule
# per binary, inbound, limited to the local subnet, so a phone on the same
# Wi-Fi reaches a test mixer and nothing on the internet does. Loopback is
# never filtered, so tests that stay on 127.0.0.1 need none of this.
#
# Run it from an elevated PowerShell after a build that added test binaries.
# It replaces its own rules each time and touches no other rule.
#
#   pwsh -File dev/windows-firewall.ps1            # add or refresh
#   pwsh -File dev/windows-firewall.ps1 -Remove    # take them all out

param([switch]$Remove)

$group = "GodwinMix dev builds"
$repo = Split-Path -Parent $PSScriptRoot
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo "target" }

Get-NetFirewallRule -Group $group -ErrorAction SilentlyContinue | Remove-NetFirewallRule
if ($Remove) { "removed the '$group' rules"; return }

$names = '^(godwinmix|gmx)[-_].*\.exe$|^(godwinmix|gmx)\.exe$'
$programs = foreach ($profile in "debug", "release") {
    foreach ($dir in (Join-Path $target $profile), (Join-Path $target "$profile\deps")) {
        if (Test-Path $dir) { Get-ChildItem $dir -Filter *.exe | Where-Object Name -match $names | ForEach-Object FullName }
    }
}
$gst = Get-Command gst-launch-1.0.exe -ErrorAction SilentlyContinue
if ($gst) { $programs += $gst.Source }

foreach ($program in $programs | Sort-Object -Unique) {
    New-NetFirewallRule -DisplayName "GodwinMix dev: $(Split-Path $program -Leaf)" -Group $group `
        -Direction Inbound -Action Allow -Program $program -RemoteAddress LocalSubnet `
        -Profile Private, Public | Out-Null
}
"allowed $(@($programs).Count) programs from the local subnet under '$group'"
