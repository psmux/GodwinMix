# One run of everything a person does on a fresh mixer, against a real core.
#
# Starts a core on a free port with a token, adds a test source, puts it on
# air, looks at it through all three doors (/api/v1, /rpc, /metrics), the
# preview and monitoring streams (/mjpeg, /pcm, /whep) and the two clients
# (gmx ctl, gmx mcp), then takes it down and checks nothing was left running.
# Every step prints ok or fails the script.
#
# The PowerShell 7 port of dev/smoke.sh. Same steps, same labels, same exit
# codes. A few steps want a POSIX shell and print "skip" with a reason on
# Windows instead of failing: the plugin steps scaffold a shell plugin, and
# the host refuses to launch one where `sh` is not a given.
#
# Usage: pwsh dev/smoke.ps1 [--keep]
#   --keep   leave the working directory and the core's log behind
#
# Needs: PowerShell 7, cargo, python3 or python (standard library only).

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# The progress bar of Invoke-WebRequest would walk all over the aligned output.
$ProgressPreference = 'SilentlyContinue'
$global:LASTEXITCODE = 0

$Repo = Split-Path -Parent $PSScriptRoot
$script:Keep = ($args.Count -ge 1 -and $args[0] -eq '--keep')

$Work = Join-Path ([System.IO.Path]::GetTempPath()) ('gmx-smoke.' + [System.IO.Path]::GetRandomFileName().Substring(0, 6))
New-Item -ItemType Directory -Path $Work -Force | Out-Null
# Start-Process cannot put both streams in one file, so the core writes two and
# everything that reads the log reads the pair.
$Log = Join-Path $Work 'core.log'
$LogErr = Join-Path $Work 'core.err.log'
$Token = 'smoke-{0}{1}' -f (Get-Random -Maximum 32768), (Get-Random -Maximum 32768)
$script:CoreProc = $null
$script:PresetProc = $null
$script:Failed = 0
$script:Fatal = $false

# --- reporting --------------------------------------------------------------

function Step {
    param([Parameter(Mandatory)][string] $Label)
    Write-Host -NoNewline ($Label.PadRight(58))
}

function Ok { Write-Host 'ok' }

# Not a failure. A step that wants a POSIX shell, a FIFO or a signal says so
# here rather than disappearing from the run.
function Skip {
    param([Parameter(Mandatory)][string] $Why)
    Write-Host "skip ($Why)"
}

function Bad {
    param([string] $Detail = '')
    Write-Host 'FAIL'
    [Console]::Error.WriteLine("    $Detail")
    $script:Failed++
}

# One step, its label and its body. The body prints ok, FAIL or skip. A body
# that throws is a FAIL and the run carries on, which is what `set -uo pipefail`
# without `-e` buys the bash version.
function Invoke-Step {
    param(
        [Parameter(Mandatory)][string] $Label,
        [Parameter(Mandatory)][scriptblock] $Body
    )
    Step $Label
    try { & $Body } catch { Bad $_.Exception.Message }
}

function Invoke-Cleanup {
    foreach ($p in @($script:CoreProc, $script:PresetProc)) {
        if ($null -eq $p) { continue }
        try {
            if (-not $p.HasExited) {
                # Windows has no SIGTERM to send, so this is the hard stop the
                # bash `kill` avoids. Nothing here needs a graceful exit.
                Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
                $p.WaitForExit(5000) | Out-Null
            }
        } catch { }
    }
    if ($script:Keep) {
        Write-Host "kept: $Work"
    } else {
        Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# --- small helpers ----------------------------------------------------------

function Get-ExeName {
    param([Parameter(Mandatory)][string] $Stem)
    if ($IsWindows) { return "$Stem.exe" }
    return $Stem
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

# python3 if it resolves, else python. The Windows store stub answers to the
# name and then does nothing, so ask it for its version before believing it.
function Resolve-Python {
    foreach ($name in @('python3', 'python')) {
        $cmd = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($null -eq $cmd) { continue }
        try {
            $probe = Invoke-Tool -File $cmd.Source -Arguments @('--version')
            if ($probe.Code -eq 0) { return $cmd.Source }
        } catch { }
    }
    return $null
}

function Format-Arg {
    param([Parameter(Mandatory)][AllowEmptyString()][string] $Value)
    if ($Value -match '[\s"]') { return '"' + ($Value -replace '"', '\"') + '"' }
    return $Value
}

# Run a program, wait for it, hand back the exit code and both streams.
# ProcessStartInfo.ArgumentList does its own quoting, which is the part
# Start-Process gets wrong on a path with a space in it.
function Invoke-Tool {
    param(
        [Parameter(Mandatory)][string] $File,
        [string[]] $Arguments = @(),
        [string] $WorkingDirectory = '',
        [hashtable] $Environment = @{},
        [string] $Log = '',
        [switch] $AppendLog
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $File
    foreach ($a in $Arguments) { $psi.ArgumentList.Add([string]$a) }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    if ($WorkingDirectory) { $psi.WorkingDirectory = $WorkingDirectory }
    foreach ($k in $Environment.Keys) { $psi.Environment[[string]$k] = [string]($Environment[$k]) }

    $p = [System.Diagnostics.Process]::Start($psi)
    # Both streams read at once. Draining one at a time deadlocks as soon as
    # the other fills its pipe.
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    $p.WaitForExit()
    $out = [string]$outTask.Result
    $err = [string]$errTask.Result
    $code = $p.ExitCode
    $p.Dispose()

    if ($Log) {
        $body = $out + $err
        if ($AppendLog) { Add-Content -LiteralPath $Log -Value $body -NoNewline }
        else { Set-Content -LiteralPath $Log -Value $body -NoNewline }
    }
    return [pscustomobject]@{ Code = $code; Out = $out; Err = $err }
}

function Write-Log {
    param([Parameter(Mandatory)][string] $Path, [AllowEmptyString()][string] $Text = '')
    Set-Content -LiteralPath $Path -Value ([string]$Text) -NoNewline
}

function Add-Log {
    param([Parameter(Mandatory)][string] $Path, [AllowEmptyString()][string] $Text = '')
    Add-Content -LiteralPath $Path -Value ([string]$Text) -NoNewline
}

function Read-Text {
    param([Parameter(Mandatory)][string] $Path)
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    $t = Get-Content -LiteralPath $Path -Raw -ErrorAction SilentlyContinue
    if ($null -eq $t) { return '' }
    return [string]$t
}

# The last few lines of a log, on one line, for a FAIL detail.
function Get-Tail {
    param([Parameter(Mandatory)][string] $Path, [int] $Count = 3)
    $text = Read-Text $Path
    if (-not $text) { return '' }
    $lines = @($text -split "`r?`n" | Where-Object { $_.Trim() })
    if ($lines.Count -gt $Count) { $lines = $lines[($lines.Count - $Count)..($lines.Count - 1)] }
    return ($lines -join '; ')
}

function Get-Matching {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string] $Text,
        [Parameter(Mandatory)][string] $Pattern,
        [int] $Count = 2
    )
    $lines = @($Text -split "`r?`n" | Where-Object { $_ -match $Pattern })
    if ($lines.Count -gt $Count) { $lines = $lines[0..($Count - 1)] }
    return ($lines -join '; ')
}

function Measure-Pattern {
    param([Parameter(Mandatory)][AllowEmptyString()][string] $Text, [Parameter(Mandatory)][string] $Pattern)
    return ([regex]::Matches($Text, $Pattern)).Count
}

function Get-Json {
    param([AllowEmptyString()][AllowNull()][string] $Text)
    if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
    try {
        # -NoEnumerate and the leading comma keep a one element JSON array an
        # array. PowerShell unrolls both on the way out otherwise, and the
        # caller then sees a single row where it wanted a list.
        $parsed = $Text | ConvertFrom-Json -AsHashtable -NoEnumerate
        return ,$parsed
    } catch { return $null }
}

# Strict mode turns a missing key into an error, so every read of parsed JSON
# goes through this.
function Get-Field {
    param([AllowNull()] $Map, [Parameter(Mandatory)][string] $Key)
    if ($null -eq $Map) { return $null }
    if ($Map -is [System.Collections.IDictionary] -and $Map.Contains($Key)) { return $Map[$Key] }
    return $null
}

# Print a value the way python's print does, so the comparisons below read the
# same in both scripts.
function Show-Value {
    param([AllowNull()] $Value)
    if ($null -eq $Value) { return 'None' }
    return [string]$Value
}

# curl's exit status and http code, out of Invoke-WebRequest. PowerShell 7
# throws on anything but a 2xx unless told not to, and throws again when the
# connection itself fails, so both ends are caught here.
function Invoke-Http {
    param(
        [Parameter(Mandatory)][string] $Uri,
        [string] $Method = 'GET',
        [hashtable] $Headers = @{},
        [AllowNull()][string] $Body = $null,
        [int] $TimeoutSec = 15
    )
    $params = @{
        Uri                = $Uri
        Method             = $Method
        TimeoutSec         = $TimeoutSec
        SkipHttpErrorCheck = $true
        ErrorAction        = 'Stop'
    }
    if ($Headers.Count -gt 0) { $params['Headers'] = $Headers }
    if (-not [string]::IsNullOrEmpty($Body)) {
        $params['Body'] = $Body
        $params['ContentType'] = 'application/json'
    }
    try {
        $r = Invoke-WebRequest @params
        $text = ''
        if ($null -ne $r.Content) {
            if ($r.Content -is [string]) { $text = $r.Content }
            elseif ($r.Content -is [byte[]]) { $text = [System.Text.Encoding]::UTF8.GetString($r.Content) }
            else { $text = [string]$r.Content }
        }
        $bytes = [byte[]]@()
        if ($null -ne $r.RawContentStream) { $bytes = $r.RawContentStream.ToArray() }
        $code = [int]$r.StatusCode
        return [pscustomobject]@{
            Ok      = ($code -ge 200 -and $code -lt 300)
            Status  = $code
            Content = $text
            Bytes   = $bytes
            Error   = ''
        }
    } catch {
        # No answer at all: curl would report 000 here.
        return [pscustomobject]@{
            Ok      = $false
            Status  = 0
            Content = ''
            Bytes   = [byte[]]@()
            Error   = $_.Exception.Message
        }
    }
}

# Processes whose command line matches, which is what pgrep -f counts.
function Get-ProcessMatchCount {
    param([Parameter(Mandatory)][string] $Pattern)
    if ($IsWindows) {
        $rows = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -and $_.CommandLine -match $Pattern })
        return $rows.Count
    }
    try {
        $res = Invoke-Tool -File 'pgrep' -Arguments @('-f', $Pattern)
        if ($res.Code -ne 0) { return 0 }
        return @($res.Out -split "`r?`n" | Where-Object { $_.Trim() }).Count
    } catch { return 0 }
}

# --- the python the config steps need ---------------------------------------
#
# The same rewriting the bash heredocs do, written out to the work directory
# and run from there. One deliberate difference: a Windows path has backslashes
# in it, which are neither a valid TOML basic string nor a safe re.sub
# replacement, so the plugins_dir is written with forward slashes.

$RewriteConfigPy = @'
import os, re, sys
src, dst, port, token = sys.argv[1:5]
text = open(src).read()
# Comment out every [[sources]] and [[outputs]] block: a smoke test drives the
# mixer through the API and must not wait on a camera that is not there.
out, skipping = [], False
for line in text.splitlines():
    stripped = line.strip()
    if stripped.startswith("[["):
        skipping = stripped in ("[[sources]]", "[[outputs]]")
    elif stripped.startswith("[") and not stripped.startswith("[["):
        skipping = False
    out.append("# " + line if skipping and not line.startswith("#") else line)
text = "\n".join(out) + "\n"
text = re.sub(r'(?m)^bind = .*$', f'bind = "127.0.0.1:{port}"', text)
text = re.sub(r'(?m)^# token = .*$', f'token = "{token}"', text)
# Short lingers, so the teardown step waits seconds rather than half a
# minute. The mechanism is what is being tested, not the shipped numbers:
# the mosaic is held up by the /rpc client and by the snapshot tracker, and
# both have to let go before it comes down.
text = re.sub(r'(?m)^linger_secs = .*$', 'linger_secs = 2', text)
text = re.sub(r'(?m)^idle_secs = .*$', 'idle_secs = 2', text)
# Plugins under the work directory, not the user's own. The plugin steps
# install into this and check it is empty again afterwards.
text = re.sub(
    r'(?m)^# plugins_dir = .*$',
    'plugins_dir = "%s/plugins"' % os.path.dirname(dst).replace("\\", "/"),
    text,
)
open(dst, "w").write(text)
'@

$RebindPy = @'
import re, sys
path, port = sys.argv[1], sys.argv[2]
text = open(path).read()
open(path, "w").write(re.sub(r'bind = "[^"]*"', f'bind = "127.0.0.1:{port}"', text))
'@

# --- the core ---------------------------------------------------------------

$Port = Get-FreePort
$Base = "http://127.0.0.1:$Port"
$Auth = @{ Authorization = "Bearer $Token" }

$GodwinMix = Join-Path $Repo 'target' 'debug' (Get-ExeName 'godwinmix')
$Gmx = Join-Path $Repo 'target' 'debug' (Get-ExeName 'gmx')

function Invoke-Smoke {
    Write-Host "GodwinMix smoke test on $Base"
    Write-Host ''

    $buildLog = Join-Path $Work 'build.log'
    Invoke-Step 'build' {
        try {
            $res = Invoke-Tool -File 'cargo' -Arguments @('build', '--quiet') -WorkingDirectory $Repo -Log $buildLog
            if ($res.Code -ne 0) { throw "cargo build exited $($res.Code)" }
        } catch {
            Bad "cargo build failed, see $buildLog"
            $script:Keep = $true
            $script:Fatal = $true
            return
        }
        Ok
    }
    if ($script:Fatal) { return }

    # The example config, with every source and output commented out: the point
    # is a core that starts clean and is driven entirely through the API.
    $exampleToml = Join-Path $Work 'example.toml'
    $configToml = Join-Path $Work 'godwinmix.toml'
    $python = Resolve-Python
    Invoke-Step 'config from --example-config' {
        if ($null -eq $python) {
            Bad 'no python3 and no python on PATH'
            $script:Fatal = $true
            return
        }
        try {
            $dumped = Invoke-Tool -File $GodwinMix -Arguments @('--example-config')
            Write-Log $exampleToml $dumped.Out
        } catch {
            Bad "godwinmix would not run: $($_.Exception.Message)"
            $script:Fatal = $true
            return
        }

        $rewriter = Join-Path $Work 'rewrite_config.py'
        [System.IO.File]::WriteAllText($rewriter, $RewriteConfigPy)
        $res = Invoke-Tool -File $python -Arguments @($rewriter, $exampleToml, $configToml, "$Port", $Token)
        if ($res.Code -ne 0) {
            Bad "the rewriter failed: $($res.Err.Trim())"
            $script:Fatal = $true
            return
        }

        $text = Read-Text $configToml
        if (($text -match '(?m)^token = ') -and ($text -match [regex]::Escape("127.0.0.1:$Port"))) {
            Ok
        } else {
            # Say what was there, so a failure on a machine nobody can sit at
            # (a CI runner) carries its own diagnosis.
            $example = Read-Text $exampleToml
            $lines = @($text -split "`n" | Where-Object { $_ -match '^(#\s*)?(bind|token) = ' })
            Bad ("the config was not rewritten: example {0} bytes, config {1} bytes, port {2}; " -f `
                $example.Length, $text.Length, $Port) + ("lines: " + ($lines -join ' | '))
            $script:Fatal = $true
        }
    }
    if ($script:Fatal) { return }

    Invoke-Step 'core starts' {
        $script:CoreProc = Start-Process -FilePath $GodwinMix `
            -ArgumentList @((Format-Arg '--config'), (Format-Arg $configToml)) `
            -WorkingDirectory $Work -PassThru -NoNewWindow `
            -RedirectStandardOutput $Log -RedirectStandardError $LogErr
        for ($i = 0; $i -lt 100; $i++) {
            if ((Invoke-Http -Uri "$Base/api/v1/core/info" -Headers $Auth -TimeoutSec 5).Ok) { break }
            if ($script:CoreProc.HasExited) { break }
            Start-Sleep -Milliseconds 200
        }
        if ((Invoke-Http -Uri "$Base/api/v1/core/info" -Headers $Auth -TimeoutSec 5).Ok) {
            Ok
        } else {
            Bad "the core never answered; see $Log and $LogErr"
            $script:Keep = $true
            [Console]::Error.WriteLine((Get-Tail $LogErr 20))
            $script:Fatal = $true
        }
    }
    if ($script:Fatal) { return }

    # --- the three doors ----------------------------------------------------

    Invoke-Step 'GET / serves the UI' {
        $r = Invoke-Http -Uri "$Base/"
        if ($r.Ok -and $r.Content -match '(?i)<!doctype html>') { Ok } else { Bad 'no page at /' }
    }

    Invoke-Step 'GET /api/v1/core/info says api_level 1' {
        $r = Invoke-Http -Uri "$Base/api/v1/core/info" -Headers $Auth
        $d = Get-Json $r.Content
        $level = Get-Field $d 'api_level'
        if ($null -ne $level -and [int]$level -eq 1) { Ok } else { Bad "core.info: $($r.Content)" }
    }

    Invoke-Step 'an unauthenticated call is refused' {
        $r = Invoke-Http -Uri "$Base/api/v1/core/status"
        if ($r.Status -eq 401) { Ok } else { Bad "expected 401, got $($r.Status)" }
    }

    # What `client/index.js` `detect()` does before it opens anything: one GET of
    # core/info, where a 200 or a 401 both mean "this core has /rpc". Getting this
    # wrong sends the UI down the deprecated /api and /ws path without saying so.
    Invoke-Step "the UI's probe picks /rpc, with or without a token" {
        $noToken = (Invoke-Http -Uri "$Base/api/v1/core/info").Status
        $withToken = (Invoke-Http -Uri "$Base/api/v1/core/info" -Headers $Auth).Status
        if ($noToken -eq 401 -and $withToken -eq 200) {
            Ok
        } else {
            Bad "core/info answered $noToken without a token and $withToken with one"
        }
    }

    Invoke-Step 'POST /api/v1/sources adds test://smpte' {
        $r = Invoke-Http -Uri "$Base/api/v1/sources" -Method POST -Headers $Auth `
            -Body '{"id":"bars","uri":"test://smpte","name":"Smoke bars"}'
        $d = $null
        if ($r.Ok) { $d = Get-Json $r.Content }
        $uri = Get-Field (Get-Field $d 'source') 'uri'
        if ($null -eq $uri) { $uri = Get-Field $d 'uri' }
        if ((Get-Field $d 'id') -eq 'bars' -and $uri) { Ok } else { Bad "source.add answered: $($r.Content)" }
    }

    # The pattern that started all this. It must be a refusal, not a dead mixer.
    Invoke-Step 'a pattern videotestsrc lacks is refused, not fatal' {
        $r = Invoke-Http -Uri "$Base/api/v1/sources" -Method POST -Headers $Auth `
            -Body '{"id":"nope","uri":"test://bars"}'
        if ($r.Content -match 'smpte' -and (Invoke-Http -Uri "$Base/api/v1/core/status" -Headers $Auth).Ok) {
            Ok
        } else {
            Bad "expected an error listing the patterns, got: $($r.Content)"
        }
    }

    Invoke-Step 'POST /api/v1/program/take puts it on air' {
        $r = Invoke-Http -Uri "$Base/api/v1/program/take" -Method POST -Headers $Auth -Body '{"source":"bars"}'
        if ($r.Ok -and $r.Content -match '"bars"') { Ok } else { Bad "program.take answered: $($r.Content)" }
    }

    Invoke-Step 'GET /api/v1/sources shows it live' {
        $listText = ''
        for ($i = 0; $i -lt 50; $i++) {
            $r = Invoke-Http -Uri "$Base/api/v1/sources" -Headers $Auth
            if ($r.Ok) { $listText = $r.Content }
            if ($listText -match '"state":"live"') { break }
            Start-Sleep -Milliseconds 200
        }
        $d = Get-Json $listText
        $rows = $d
        if ($d -is [System.Collections.IDictionary]) { $rows = Get-Field $d 'sources' }
        $one = @(@($rows) | Where-Object { $null -ne $_ -and (Get-Field $_ 'id') -eq 'bars' })
        if ($one.Count -gt 0 -and (Get-Field $one[0] 'state') -eq 'live') {
            Ok
        } else {
            Bad "source.list answered: $listText"
        }
    }

    Invoke-Step '/metrics carries gmx_programme_frame_interval_ms' {
        $r = Invoke-Http -Uri "$Base/metrics"
        if ($r.Ok -and $r.Content -match 'gmx_programme_frame_interval_ms') {
            Ok
        } else {
            Bad 'the histogram is not in the scrape'
        }
    }

    $sheet = Join-Path $Work 'sheet.jpg'
    Invoke-Step 'GET /api/v1/snapshot/sheet returns a JPEG' {
        $r = Invoke-Http -Uri "$Base/api/v1/snapshot/sheet?width=320" -Headers $Auth
        if ($r.Ok -and $r.Bytes.Count -gt 0) { [System.IO.File]::WriteAllBytes($sheet, $r.Bytes) }
        # ffd8 is the JPEG start of image marker, which is what the xxd in the
        # bash version is looking at.
        if ($r.Ok -and $r.Bytes.Count -gt 1 -and $r.Bytes[0] -eq 0xFF -and $r.Bytes[1] -eq 0xD8) {
            Ok
        } else {
            Bad 'no JPEG came back'
        }
    }

    # --- /rpc ---------------------------------------------------------------

    $rpcLog = Join-Path $Work 'rpc.log'
    Invoke-Step '/rpc: subscribe, snapshot, flush, a mosaic frame' {
        $res = Invoke-Tool -File $python -Arguments @((Join-Path $Repo 'dev' 'smoke_rpc.py'), '127.0.0.1', "$Port", $Token) -Log $rpcLog
        if ($res.Code -eq 0) { Ok } else { Bad (Get-Tail $rpcLog 5) }
    }

    Invoke-Step 'the mosaic goes after the linger' {
        $down = $false
        $subs = ''
        for ($i = 0; $i -lt 40; $i++) {
            $r = Invoke-Http -Uri "$Base/metrics"
            foreach ($line in ($r.Content -split "`r?`n")) {
                if ($line -match '^gmx_multiview_subscribers') { $subs = @($line -split '\s+')[1] }
            }
            if ($subs -eq '0') { $down = $true; break }
            Start-Sleep -Milliseconds 250
        }
        if ($down) {
            Ok
        } else {
            $seen = if ($subs) { $subs } else { 'unknown' }
            Bad "gmx_multiview_subscribers stuck at $seen"
        }
    }

    $streamsLog = Join-Path $Work 'streams.log'
    Invoke-Step 'preview and audio: /mjpeg, /pcm, /whep' {
        $res = Invoke-Tool -File $python -Arguments @((Join-Path $Repo 'dev' 'smoke_streams.py'), '127.0.0.1', "$Port", $Token) -Log $streamsLog
        if ($res.Code -eq 0) { Ok } else { Bad ((Read-Text $streamsLog) -replace "`r?`n", '; ') }
    }

    Invoke-Step 'and the log says the mosaic was torn down' {
        $text = (Read-Text $Log) + (Read-Text $LogErr)
        if ($text -match '(?i)multiview|mosaic') { Ok } else { Bad 'nothing in the log about the mosaic' }
    }

    # --- presets ------------------------------------------------------------
    #
    # The volunteer's install, on a directory that has nothing in it: the plan
    # first, then the real thing, then a core started from what it wrote.

    $pwork = Join-Path $Work 'preset'
    New-Item -ItemType Directory -Path $pwork -Force | Out-Null
    $presetCfg = Join-Path $pwork 'godwinmix.toml'
    $presetScenes = Join-Path $pwork 'godwinmix.scenes.json'
    $presetRuntime = Join-Path $pwork 'godwinmix.runtime.toml'

    Invoke-Step 'gmx preset list names the six official presets' {
        $res = Invoke-Tool -File $Gmx -Arguments @('preset', 'list', '--json')
        Write-Log (Join-Path $Work 'preset.log') $res.Err
        $rows = Get-Json $res.Out
        $listed = ''
        if ($null -ne $rows) {
            $names = @(@($rows) | Where-Object { Get-Field $_ 'official' } | ForEach-Object { [string](Get-Field $_ 'name') })
            $listed = (($names | Sort-Object) -join ',')
        }
        if ($listed -eq 'broadcast,church,classroom,default,esports,headless-agent') {
            Ok
        } else {
            $seen = if ($listed) { $listed } else { 'nothing' }
            Bad "listed $seen"
        }
    }

    $dryLog = Join-Path $Work 'dry.log'
    Invoke-Step 'preset apply --dry-run names the camera plugin' {
        Invoke-Tool -File $Gmx -Arguments @('preset', 'apply', 'church', '--dry-run', '--config', $presetCfg) -Log $dryLog | Out-Null
        if ((Read-Text $dryLog) -match 'MISSING  camera' -and -not (Test-Path -LiteralPath $presetCfg)) {
            Ok
        } else {
            Bad (Get-Tail $dryLog 3)
        }
    }

    Invoke-Step 'and nothing else in the plan is wrong' {
        $text = Read-Text $dryLog
        if ($text -match '(?i)error|does not (load|apply|resolve|validate)') {
            Bad (Get-Matching $text '(?i)error|does not' 2)
        } else {
            Ok
        }
    }

    $applyLog = Join-Path $Work 'apply.log'
    Invoke-Step 'preset apply writes config, scenes and [ui]' {
        Invoke-Tool -File $Gmx -Arguments @('preset', 'apply', 'church', '--config', $presetCfg) -Log $applyLog | Out-Null
        if ((Test-Path -LiteralPath $presetCfg) -and (Test-Path -LiteralPath $presetScenes) `
                -and ((Read-Text $presetRuntime) -match '(?m)^\[ui\]')) {
            Ok
        } else {
            Bad (Get-Tail $applyLog 3)
        }
    }

    Invoke-Step "the preset's own comments came with its config" {
        if ((Read-Text $presetCfg) -match '# The church preset') { Ok } else { Bad 'the comments were lost' }
    }

    Invoke-Step 'applying it twice does not double the sources' {
        $before = Measure-Pattern (Read-Text $presetCfg) '(?m)^\[\[sources\]\]'
        Invoke-Tool -File $Gmx -Arguments @('preset', 'apply', 'church', '--config', $presetCfg) -Log $applyLog -AppendLog | Out-Null
        $after = Measure-Pattern (Read-Text $presetCfg) '(?m)^\[\[sources\]\]'
        if ($before -eq $after) { Ok } else { Bad "$before sources became $after" }
    }

    $pport = Get-FreePort
    $presetCoreLog = Join-Path $Work 'preset-core.log'
    $presetCoreErr = Join-Path $Work 'preset-core.err.log'
    $pinfo = Join-Path $Work 'pinfo.json'
    Invoke-Step 'a core starts from what the preset wrote' {
        $rebind = Join-Path $Work 'rebind.py'
        [System.IO.File]::WriteAllText($rebind, $RebindPy)
        Invoke-Tool -File $python -Arguments @($rebind, $presetCfg, "$pport") | Out-Null
        $script:PresetProc = Start-Process -FilePath $GodwinMix `
            -ArgumentList @((Format-Arg '--config'), (Format-Arg $presetCfg)) `
            -PassThru -NoNewWindow `
            -RedirectStandardOutput $presetCoreLog -RedirectStandardError $presetCoreErr
        $up = $false
        for ($i = 0; $i -lt 80; $i++) {
            $r = Invoke-Http -Uri "http://127.0.0.1:$pport/api/v1/core/info" -TimeoutSec 5
            if ($r.Ok) {
                Write-Log $pinfo $r.Content
                $up = $true
                break
            }
            Start-Sleep -Milliseconds 250
        }
        if ($up) { Ok } else { Bad "the core did not come up: $(Get-Tail $presetCoreErr 3)" }
    }

    Invoke-Step "core.info carries the preset's theme and gallery" {
        $d = Get-Json (Read-Text $pinfo)
        $ui = Get-Field $d 'ui'
        $layout = Get-Field $ui 'layout'
        $count = 0
        if ($layout -is [System.Collections.IDictionary]) { $count = $layout.Keys.Count }
        $line = '{0} {1} {2} {3}' -f (Show-Value (Get-Field $ui 'preset')), (Show-Value (Get-Field $ui 'theme')), `
            (Show-Value (Get-Field $ui 'gallery')), $count
        if ($line -eq 'church calm icon 4') { Ok } else { Bad "core.info ui is '$line'" }
    }

    Invoke-Step "the preset's theme is served over HTTP" {
        $r = Invoke-Http -Uri "http://127.0.0.1:$pport/presets/church/theme.css"
        if ($r.Ok -and $r.Content -match '--live') {
            Ok
        } else {
            Bad 'no stylesheet at /presets/church/theme.css'
        }
    }

    $pstatusLog = Join-Path $Work 'pstatus.log'
    Invoke-Step "ctl status: the preset's sources and outputs" {
        Invoke-Tool -File $Gmx -Arguments @('ctl', 'status') `
            -Environment @{ GODWINMIX_URL = "http://127.0.0.1:$pport" } -Log $pstatusLog | Out-Null
        $text = Read-Text $pstatusLog
        if ($text -match 'cam-wide' -and $text -match 'youtube') {
            Ok
        } else {
            Bad ($text -replace "`r?`n", '; ')
        }
    }

    Invoke-Step 'preset.apply over the API returns the plan' {
        $r = Invoke-Http -Uri "http://127.0.0.1:$pport/api/v1/preset/apply" -Method POST `
            -Body '{"name":"church","dry_run":true}'
        $applied = ''
        if ($r.Ok) {
            $d = Get-Json $r.Content
            $steps = Get-Field (Get-Field $d 'plan') 'steps'
            $dryRun = Get-Field $d 'dry_run'
            if ($null -ne $dryRun -and $null -ne $steps) {
                $applied = '{0} {1}' -f (Show-Value $dryRun), @($steps).Count
            }
        }
        if ($applied -eq 'True 3') {
            Ok
        } else {
            $seen = if ($applied) { $applied } else { 'nothing' }
            Bad "preset.apply answered '$seen'"
        }
    }

    $saveLog = Join-Path $Work 'save.log'
    $showLog = Join-Path $Work 'show.log'
    Invoke-Step 'preset save writes one the loader reads back' {
        $mine = Join-Path $pwork 'mine'
        Invoke-Tool -File $Gmx -Arguments @('preset', 'save', 'my-church', '--config', $presetCfg, '--out', $mine) -Log $saveLog | Out-Null
        $shown = Invoke-Tool -File $Gmx -Arguments @('preset', 'show', $mine, '--config', (Join-Path $pwork 'second.toml')) -Log $showLog
        $saved = Read-Text (Join-Path $mine 'config' 'godwinmix.toml')
        if ($shown.Code -eq 0 -and $saved -match 'YOUR-STREAM-KEY' -and $saved -notmatch 'token = "') {
            Ok
        } else {
            Bad ("$(Get-Tail $saveLog 3) $(Get-Tail $showLog 3)")
        }
    }

    $build2Log = Join-Path $Work 'build2.log'
    Invoke-Step 'gmx build refuses to bundle a copyleft codec entry' {
        $buildOut = Join-Path $pwork 'build'
        Invoke-Tool -File $Gmx -Arguments @('build', '--preset', 'church', '--name', 'SmokeMix', '--out', $buildOut, '--no-binary') -Log $build2Log | Out-Null
        if ((Read-Text $build2Log) -match 'copyleft' `
                -and (Test-Path -LiteralPath (Join-Path $buildOut 'tauri.conf.json')) `
                -and ((Read-Text (Join-Path $buildOut 'codecs.toml')) -notmatch 'GPL-2\.0')) {
            Ok
        } else {
            Bad (Get-Tail $build2Log 3)
        }
    }

    if ($null -ne $script:PresetProc) {
        try {
            if (-not $script:PresetProc.HasExited) {
                Stop-Process -Id $script:PresetProc.Id -Force -ErrorAction SilentlyContinue
                $script:PresetProc.WaitForExit(5000) | Out-Null
            }
        } catch { }
        $script:PresetProc = $null
    }

    # --- the clients --------------------------------------------------------

    $coreEnv = @{ GODWINMIX_URL = $Base; GODWINMIX_TOKEN = $Token }

    $ctlLog = Join-Path $Work 'ctl.log'
    Invoke-Step 'gmx ctl status' {
        Invoke-Tool -File $Gmx -Arguments @('ctl', 'status') -Environment $coreEnv -Log $ctlLog | Out-Null
        $text = Read-Text $ctlLog
        $oneLine = $text -replace "`r?`n", '; '
        if ($text -match 'bars') {
            Ok
        } else {
            Bad "ctl status did not list the source: $oneLine"
        }
    }

    # --- a plugin, installed and removed while the programme runs -----------
    # The whole tier 2 path in one step: a plugin written in shell, installed into a
    # live core, producing real frames at canvas caps, then removed with nothing
    # left behind. If this passes, a third party can write a source.
    #
    # The scaffold writes a run.sh and the host refuses a shell plugin on
    # Windows, where `sh` is not a given, so these five steps say skip there.
    $plugDir = Join-Path $Work 'smoke-bars'
    $pluginTestLog = Join-Path $Work 'plugin-test.log'
    Invoke-Step 'gmx plugin new writes a plugin that passes the harness' {
        if ($IsWindows) { Skip 'a shell plugin needs sh'; return }
        $newLog = Join-Path $Work 'plugin-new.log'
        $made = Invoke-Tool -File $Gmx -Arguments @('plugin', 'new', 'smoke-bars', '--kind', 'source', '--lang', 'shell', '--out', $plugDir) -Log $newLog
        $tested = [pscustomobject]@{ Code = 1 }
        if ($made.Code -eq 0) {
            $tested = Invoke-Tool -File $Gmx -Arguments @('plugin', 'test', $plugDir, '--quick') -Log $pluginTestLog
        }
        if ($made.Code -eq 0 -and $tested.Code -eq 0) {
            Ok
        } else {
            Bad "the generated plugin did not pass: $(Get-Tail $pluginTestLog 5)"
        }
    }

    $offlineLog = Join-Path $Work 'plugin-offline.log'
    Invoke-Step 'gmx plugin test --offline replays its transcript with no core' {
        if ($IsWindows) { Skip 'a shell plugin needs sh'; return }
        $res = Invoke-Tool -File $Gmx -Arguments @('plugin', 'test', $plugDir, '--offline') -Log $offlineLog
        if ($res.Code -eq 0) {
            Ok
        } else {
            Bad "the offline replay failed: $(Get-Tail $offlineLog 3)"
        }
    }

    $addLog = Join-Path $Work 'plugin-add.log'
    Invoke-Step 'gmx plugin add installs it into the running core' {
        if ($IsWindows) { Skip 'no shell plugin to install'; return }
        $res = Invoke-Tool -File $Gmx -Arguments @('plugin', 'add', $plugDir) -Environment $coreEnv -Log $addLog
        $text = Read-Text $addLog
        $oneLine = $text -replace "`r?`n", '; '
        if ($res.Code -eq 0 -and $text -match 'smoke-bars/source') {
            Ok
        } else {
            Bad "plugin add failed: $oneLine"
        }
    }

    $pluginListLog = Join-Path $Work 'plugin-list.log'
    Invoke-Step 'its source goes live and carries its cost in plugin list' {
        if ($IsWindows) { Skip 'no shell plugin to run'; return }
        Invoke-Tool -File $Gmx -Arguments @('ctl', 'source', 'add', 'plugbars', '--type', 'smoke-bars/source') `
            -Environment $coreEnv -Log (Join-Path $Work 'plugin-source.log') | Out-Null
        $live = $false
        for ($i = 0; $i -lt 10; $i++) {
            Start-Sleep -Seconds 1
            $status = Invoke-Tool -File $Gmx -Arguments @('ctl', 'status') -Environment $coreEnv
            if ($status.Out -match 'plugbars.*live') { $live = $true; break }
        }
        Invoke-Tool -File $Gmx -Arguments @('plugin', 'list') -Environment $coreEnv -Log $pluginListLog | Out-Null
        $text = Read-Text $pluginListLog
        $oneLine = $text -replace "`r?`n", '; '
        if ($live -and $text -match 'plugbars') {
            Ok
        } else {
            Bad "the plugin source did not go live: $oneLine"
        }
    }

    Invoke-Step 'gmx plugin remove leaves no process and no directory' {
        if ($IsWindows) { Skip 'no shell plugin to remove'; return }
        Invoke-Tool -File $Gmx -Arguments @('ctl', 'source', 'remove', 'plugbars') -Environment $coreEnv | Out-Null
        Start-Sleep -Seconds 1
        Invoke-Tool -File $Gmx -Arguments @('plugin', 'remove', 'smoke-bars') `
            -Environment $coreEnv -Log (Join-Path $Work 'plugin-remove.log') | Out-Null
        Start-Sleep -Seconds 2
        $strays = Get-ProcessMatchCount 'smoke-bars/0.1.0/run.sh'
        $installed = Join-Path $Work 'plugins' 'smoke-bars'
        $left = 0
        if (Test-Path -LiteralPath $installed) {
            $left = @(Get-ChildItem -LiteralPath $installed -Force -ErrorAction SilentlyContinue).Count
        }
        if ($strays -eq 0 -and $left -eq 0) {
            Ok
        } else {
            Bad "removal left $strays process(es) and $left directory entr(ies)"
        }
    }

    $mcpLog = Join-Path $Work 'mcp.log'
    $mcpPy = Join-Path $Repo 'dev' 'smoke_mcp.py'
    Invoke-Step 'gmx mcp lists 12 tools on standard' {
        $res = Invoke-Tool -File $python -Arguments @($mcpPy, $Gmx, $Base, $Token, 'standard')
        Add-Log $mcpLog $res.Err
        $count = $res.Out.Trim()
        if ($count -eq '12') {
            Ok
        } else {
            $seen = if ($count) { $count } else { 'nothing' }
            Bad "standard listed $seen, wanted 12"
        }
    }

    Invoke-Step 'gmx mcp lists 5 tools on minimal' {
        $res = Invoke-Tool -File $python -Arguments @($mcpPy, $Gmx, $Base, $Token, 'minimal')
        Add-Log $mcpLog $res.Err
        $count = $res.Out.Trim()
        if ($count -eq '5') {
            Ok
        } else {
            $seen = if ($count) { $count } else { 'nothing' }
            Bad "minimal listed $seen, wanted 5"
        }
    }

    # --- shutdown -----------------------------------------------------------

    Invoke-Step 'core.shutdown stops the process' {
        Invoke-Http -Uri "$Base/api/v1/core/shutdown" -Method POST -Headers $Auth -Body '{}' | Out-Null
        $gone = $false
        for ($i = 0; $i -lt 60; $i++) {
            if ($script:CoreProc.HasExited) { $gone = $true; break }
            Start-Sleep -Milliseconds 250
        }
        if ($gone) { Ok } else { Bad 'the core is still running after core.shutdown' }
        if ($gone) {
            $script:CoreProc.WaitForExit(5000) | Out-Null
            $script:CoreProc = $null
        }
    }

    Invoke-Step 'the log has no panic and no unclosed delimiter' {
        $text = (Read-Text $Log) + (Read-Text $LogErr)
        $pattern = '(?i)panicked at|unclosed delimiter|has no property'
        if ($text -match $pattern) { Bad (Get-Matching $text $pattern 3) } else { Ok }
    }

    Invoke-Step 'nothing was left running' {
        $left = Get-ProcessMatchCount ('godwinmix-browser|godwinmix --config ' + [regex]::Escape($Work))
        if ($left -eq 0) { Ok } else { Bad "$left stray process(es)" }
    }

    Write-Host ''
    if ($script:Failed -eq 0) {
        Write-Host 'all steps ok'
        return
    }
    Write-Host "$($script:Failed) step(s) failed"
    $script:Keep = $true
}

try {
    Invoke-Smoke
} catch {
    Bad "the run stopped: $($_.Exception.Message)"
    $script:Keep = $true
} finally {
    Invoke-Cleanup
}

if ($script:Failed -gt 0) { exit 1 }
exit 0
