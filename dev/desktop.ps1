# Open the desktop app on Windows, starting the mixer first if none is running.
#
#   dev\desktop.ps1 mine.toml     mixer on your config, then the app
#   dev\desktop.ps1               the app only, against a mixer already on :8080
#
# The Windows counterpart of dev/desktop.sh. The test rig (mediamtx, the
# synthetic camera, the page server) is a bash affair and is not started here;
# point the config at real sources, or run the rig under WSL.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Api = "http://127.0.0.1:8080/api/status"

function MixerUp { try { Invoke-RestMethod -Uri $Api -TimeoutSec 2 | Out-Null; $true } catch { $false } }

if (-not (MixerUp)) {
  if ($args.Count -lt 1) { Write-Error "no mixer on :8080; give a config file: dev\desktop.ps1 mine.toml"; exit 1 }
  $cfg = $args[0]
  New-Item -ItemType Directory -Force -Path "$Root\dev\harness\logs" | Out-Null
  Write-Host "starting the mixer on $cfg"
  Start-Process -FilePath "$Root\target\release\godwinmix.exe" -ArgumentList @("--config", $cfg) `
    -RedirectStandardOutput "$Root\dev\harness\logs\mixer.log" -RedirectStandardError "$Root\dev\harness\logs\mixer.err.log" -WindowStyle Hidden
  $waited = 0
  while (-not (MixerUp) -and $waited -lt 30) { Start-Sleep -Seconds 1; $waited++ }
  if (-not (MixerUp)) { Write-Error "the mixer did not come up; see dev\harness\logs\mixer.err.log"; exit 1 }
}

$App = "$Root\tauri-app\target\release\godwinmix-desktop.exe"
if (-not (Test-Path $App)) { Write-Error "no desktop build yet: cd tauri-app; cargo build --release"; exit 1 }
Write-Host "desktop app open against http://localhost:8080 (this waits until it quits)"
$p = Start-Process -FilePath $App -PassThru -Wait
# "Quit and stop the mixer" exits with 2 after asking the mixer to shut down;
# plain quit, or closing the window, leaves the mixer running.
if ($p.ExitCode -eq 2) { Write-Host "mixer stopped" }
exit 0
