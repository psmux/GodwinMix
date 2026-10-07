# Checks what a release would hand a Windows user, and fails if any of it is
# unsigned. Each file named is checked with `signtool verify /pa`. An .msi is
# unpacked with an administrative install and a -setup.exe with 7-Zip, and
# every .exe and .dll inside must carry a valid signature too. The first
# party executables (godwinmix*, gmx*) must be signed by the certificate
# login.ps1 found, not just by somebody.
#
# The first file named gets the full `signtool verify /pa /v` output, which
# shows the signer, the issuer chain and the timestamp.
param([Parameter(Mandatory, ValueFromRemainingArguments)] [object[]] $Files)
# Lists passed as one argument are flattened, so both `a b` and `@(a, b)` work.
$Files = @($Files | ForEach-Object { $_ } | ForEach-Object { [string]$_ })
$ErrorActionPreference = 'Stop'
$bad = [System.Collections.Generic.List[string]]::new()

function Test-Inner([string] $Dir, [string] $From) {
    $all = @(Get-ChildItem $Dir -Recurse -File -Include *.exe, *.dll)
    foreach ($f in $all) {
        $sig = Get-AuthenticodeSignature $f.FullName
        $ours = $f.Name -like 'godwinmix*' -or $f.Name -like 'gmx*'
        if ($sig.Status -ne 'Valid') {
            $bad.Add("$From > $($f.Name): $($sig.Status)")
        } elseif ($ours -and $sig.SignerCertificate.Thumbprint -ne $env:SIGN_THUMBPRINT) {
            $bad.Add("$From > $($f.Name): signed by $($sig.SignerCertificate.Subject), not ours")
        } elseif ($ours) {
            Write-Host "  signed: $($f.Name)"
        }
    }
    Write-Host "  $($all.Count) executables and libraries inside $From"
}

$first = $true
foreach ($file in $Files) {
    $path = (Resolve-Path $file).Path
    $name = Split-Path $path -Leaf
    Write-Host "== $name"
    if ($first) { & $env:SIGNTOOL verify /pa /v $path } else { & $env:SIGNTOOL verify /pa /q $path }
    if ($LASTEXITCODE -ne 0) { $bad.Add("$name`: signtool verify /pa failed") }
    $first = $false
    $out = Join-Path $env:RUNNER_TEMP ("unpacked-" + [IO.Path]::GetFileNameWithoutExtension($name))
    if ($name -like '*.msi') {
        $p = Start-Process msiexec.exe -ArgumentList "/a `"$path`" /qn TARGETDIR=`"$out`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { $bad.Add("$name`: msiexec /a exited $($p.ExitCode)"); continue }
        Test-Inner $out $name
    } elseif ($name -like '*-setup.exe') {
        & 7z x -y "-o$out" $path | Out-Null
        if ($LASTEXITCODE -ne 0) { $bad.Add("$name`: 7z could not unpack it"); continue }
        Test-Inner $out $name
    }
}
if ($bad.Count) {
    $bad | ForEach-Object { Write-Host "::error::not signed: $_" }
    exit 1
}
Write-Host "every file is signed"
