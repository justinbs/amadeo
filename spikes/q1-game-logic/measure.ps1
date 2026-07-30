# Re-runs the whole Q1 measurement and prints the table that ADR 0011 quotes.
#
#   pwsh -File measure.ps1            # everything
#   pwsh -File measure.ps1 -Quick     # skip the rebuild-latency samples (the slow part)
#
# Numbers in ADR 0011 came from this script on a Ryzen 7 5700X3D / Windows 11 / rustc 1.97.1.
# Re-run it before trusting them on any other machine, or after the engine has grown.

param([switch]$Quick)

# Deliberately NOT "Stop": cargo writes its progress to stderr, and Windows PowerShell turns a
# native command's stderr into ErrorRecords. With "Stop" the first `Compiling ...` line aborts the
# script. Failures are detected by checking $LASTEXITCODE instead.
$ErrorActionPreference = "Continue"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$root = $PSScriptRoot
Set-Location $root

$utf8NoBom = New-Object System.Text.UTF8Encoding $false

function Measure-Rebuild {
    param([string]$File, [string]$Cmd, [string]$WorkDir, [int]$Runs = 5)

    Push-Location $WorkDir
    $full = (Resolve-Path $File).Path
    # .NET file APIs, not Get-Content/Set-Content: PowerShell 5.1 reads UTF-8 as ANSI and writes
    # back with a BOM, which corrupts every non-ASCII character in the file.
    $original = [System.IO.File]::ReadAllText($full)
    $samples = @()
    try {
        for ($i = 1; $i -le $Runs; $i++) {
            [System.IO.File]::WriteAllText($full, $original + "`n// spike touch $i`n", $utf8NoBom)
            $sw = [Diagnostics.Stopwatch]::StartNew()
            # No `2>&1`: redirecting a native command's stderr in PowerShell 5.1 wraps each line
            # in an ErrorRecord and can flip $? even on a clean exit.
            $null = Invoke-Expression $Cmd
            $sw.Stop()
            if ($LASTEXITCODE -eq 0) { $samples += $sw.Elapsed.TotalSeconds }
        }
    }
    finally {
        [System.IO.File]::WriteAllText($full, $original, $utf8NoBom)
        Pop-Location
    }
    if ($samples.Count -eq 0) { return $null }
    return [math]::Round(($samples | Sort-Object)[[math]::Floor($samples.Count / 2)], 3)
}

function Get-Field {
    param([string[]]$Lines, [string]$Pattern)
    $match = $Lines | Select-String $Pattern | Select-Object -First 1
    if ($match) { return $match.Line.Trim() }
    return ""
}

Write-Output "Building all candidates (debug + release)..."
cargo build -q -p a-rust -p b-cdylib-logic -p b-cdylib-host -p c-luau -p d-wasm-host
cargo build -q --release -p a-rust -p b-cdylib-logic -p b-cdylib-host -p c-luau -p d-wasm-host
Push-Location "$root\d-wasm-logic"
cargo build -q --release --target wasm32-unknown-unknown
Pop-Location

Write-Output ""
Write-Output "=============================================================="
Write-Output " 1. AGREEMENT -- state hash after 1800 ticks, 64 enemies"
Write-Output "=============================================================="
$results = @{}
foreach ($pair in @(
        @("A pure Rust", "$root\target\release\a-rust.exe", @()),
        @("B cdylib", "$root\target\release\b-cdylib-host.exe", @()),
        @("C Luau", "$root\target\release\c-luau.exe", @()),
        @("D WASM", "$root\target\release\d-wasm-host.exe", @()))) {
    $lines = & $pair[1] @($pair[2]) 2>&1 | ForEach-Object { "$_" }
    $hash = ($lines | Select-String 'state_hash (\w+)').Matches.Groups[1].Value
    $us = ($lines | Select-String '\(([\d.]+) us/tick\)').Matches.Groups[1].Value
    $results[$pair[0]] = @{ hash = $hash; us = $us }
    Write-Output ("  {0,-12} {1}  {2,7} us/tick" -f $pair[0], $hash, $us)
}
$reference = $results["A pure Rust"].hash
Write-Output ""
foreach ($key in @("B cdylib", "C Luau", "D WASM")) {
    $verdict = if ($results[$key].hash -eq $reference) { "MATCHES native Rust" } else { "DIFFERS from native Rust" }
    Write-Output ("  {0,-12} {1}" -f $key, $verdict)
}

Write-Output ""
Write-Output "=============================================================="
Write-Output " 2. STATE SURVIVAL -- reload at tick 900 of an 1800-tick run"
Write-Output "=============================================================="
Write-Output "  (final hash must still equal the uninterrupted run's)"
foreach ($pair in @(
        @("B cdylib", "$root\target\release\b-cdylib-host.exe"),
        @("C Luau", "$root\target\release\c-luau.exe"),
        @("D WASM", "$root\target\release\d-wasm-host.exe"))) {
    $lines = & $pair[1] --reload-at 900 2>&1 | ForEach-Object { "$_" }
    $survived = if ($lines | Select-String 'state survived') { "state survived" } else { "STATE LOST" }
    $final = ($lines | Select-String 'tick 1800 \| state_hash (\w+)').Matches.Groups[1].Value
    Write-Output ("  {0,-12} {1,-15} final {2}" -f $pair[0], $survived, $final)
}
Write-Output ("  {0,-12} {1}" -f "A pure Rust", "n/a -- no reload mechanism; the process restarts")

Write-Output ""
Write-Output "=============================================================="
Write-Output " 3. RELOAD SWAP -- the in-process part of edit->observe"
Write-Output "=============================================================="
foreach ($pair in @(
        @("B cdylib", "$root\target\release\b-cdylib-host.exe", 20),
        @("C Luau", "$root\target\release\c-luau.exe", 50),
        @("D WASM", "$root\target\release\d-wasm-host.exe", 20))) {
    $lines = & $pair[1] --reload-samples $pair[2] --ticks 60 2>&1 | ForEach-Object { "$_" }
    Write-Output ("  {0,-12} {1}" -f $pair[0], (Get-Field $lines 'reload swap'))
    $jit = Get-Field $lines 'of which JIT'
    if ($jit) { Write-Output ("  {0,-12} {1}" -f "", $jit) }
}

Write-Output ""
Write-Output "=============================================================="
Write-Output " 4. LUAU COST BREAKDOWN -- language vs binding"
Write-Output "=============================================================="
$full = (& "$root\target\release\c-luau.exe" --script enemy 2>&1 | Select-String '\(([\d.]+) us/tick\)').Matches.Groups[1].Value
$null_ = (& "$root\target\release\c-luau.exe" --script null 2>&1 | Select-String '\(([\d.]+) us/tick\)').Matches.Groups[1].Value
Write-Output ("  full script          : {0} us/tick" -f $full)
Write-Output ("  do-nothing script    : {0} us/tick   <- marshalling alone" -f $null_)
Write-Output ("  Luau executing the AI: {0} us/tick" -f [math]::Round([double]$full - [double]$null_, 1))

Write-Output ""
Write-Output "=============================================================="
Write-Output " 5. RE-SIMULATION COST -- what candidate A pays instead"
Write-Output "=============================================================="
foreach ($t in @(1800, 7200, 18000)) {
    $samples = @()
    for ($i = 0; $i -lt 5; $i++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $null = & "$root\target\debug\a-rust.exe" $t 2>&1
        $sw.Stop()
        $samples += $sw.Elapsed.TotalMilliseconds
    }
    $median = [math]::Round(($samples | Sort-Object)[2], 1)
    Write-Output ("  restart + {0,6} ticks ({1,4}s simulated) : {2} ms" -f $t, ($t / 60), $median)
}

if ($Quick) {
    Write-Output ""
    Write-Output "(skipped section 6: rebuild latency -- re-run without -Quick)"
    return
}

Write-Output ""
Write-Output "=============================================================="
Write-Output " 6. REBUILD LATENCY -- median of 5, after a one-line edit"
Write-Output "=============================================================="
$a = Measure-Rebuild -File "a-rust\src\ai.rs" -Cmd "cargo build -p a-rust" -WorkDir $root
Write-Output ("  A  gameplay crate rebuild      : {0} s" -f $a)
$b = Measure-Rebuild -File "b-cdylib-logic\src\lib.rs" -Cmd "cargo build -p b-cdylib-logic" -WorkDir $root
Write-Output ("  B  cdylib rebuild              : {0} s" -f $b)
Write-Output ("  C  script edit                 : 0 s (nothing is compiled)")
$d = Measure-Rebuild -File "src\lib.rs" -Cmd "cargo build --release --target wasm32-unknown-unknown" -WorkDir "$root\d-wasm-logic"
Write-Output ("  D  wasm guest rebuild          : {0} s" -f $d)

Write-Output ""
Write-Output "  For comparison, in the ENGINE workspace:"
$engine = "$root\..\.."
$game = Measure-Rebuild -File "games\quad-demo\src\main.rs" -Cmd "cargo build -p quad-demo" -WorkDir $engine
Write-Output ("    quad-demo, gameplay edit     : {0} s  (links wgpu + winit)" -f $game)
$core = Measure-Rebuild -File "crates\amadeo-ecs\src\world.rs" -Cmd "cargo build -p quad-demo" -WorkDir $engine -Runs 3
Write-Output ("    quad-demo, amadeo-ecs edit   : {0} s  (rebuilds everything downstream)" -f $core)
