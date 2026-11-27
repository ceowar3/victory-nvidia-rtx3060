param(
    [Parameter(Mandatory = $true)][string]$GameExe,
    [switch]$VerboseLogs,
    [switch]$LegacyDescriptors
)
$ErrorActionPreference = 'Stop'
$game = (Resolve-Path -LiteralPath $GameExe).Path
if (-not (Test-Path -LiteralPath $game -PathType Leaf)) { throw 'Game executable not found.' }
$binary = Join-Path $PSScriptRoot 'victory.exe'
if (-not (Test-Path -LiteralPath $binary)) { $binary = Join-Path $PSScriptRoot 'target\release\victory.exe' }
if (-not (Test-Path -LiteralPath $binary)) { throw 'Build first: cargo build --release --locked -p victory' }
$old = @{}
foreach ($entry in @(Get-ChildItem Env:VICTORY_*)) {
    $old[$entry.Name] = $entry.Value
    Remove-Item -LiteralPath ('Env:' + $entry.Name)
}
$runData = Join-Path $env:LOCALAPPDATA 'VIctory-RTX3060'
try {
    $env:VICTORY_GAME_DIR = Split-Path -Parent $game
    $env:VICTORY_CACHE_DIR = Join-Path $runData 'cache'
    $env:VICTORY_DATA_DIR = Join-Path $runData 'data'
    $env:VICTORY_WAVE_PAIRS = '1'
    $env:VICTORY_PIPELINE_WORKERS = '16'
    $env:VICTORY_FAST_CORES = '1'
    $env:VICTORY_DISPLAY_MODE = '1920x1080'
    $env:VICTORY_RELAY = '2'
    $env:VICTORY_LATE_WAIT = '3'
    $env:VICTORY_WIDE_BACK = '1'
    $env:VICTORY_CARD_BUFFERS_MIN_MB = '1'
    $env:VICTORY_CARD_BUFFERS_MB = '1024'
    $env:VICTORY_ASYNC_SUBMIT = '1'
    $env:VICTORY_QUEUE2 = '1'
    $env:VICTORY_INDIRECT_WINDOW = '1'
    $env:VICTORY_CARD_FETCH_INFLIGHT = '0'
    $env:VICTORY_TEXTURE_NO_WAIT = '1'
    $env:VICTORY_MERGE_RUNS = '4096'
    $env:VICTORY_MERGE_GAP = '0'
    $env:VICTORY_TEXTURE_BACK = '1'
    if (-not $LegacyDescriptors) { $env:VICTORY_DYNAMIC_SLOTS = '524288' }
    $env:VICTORY_LOG = if ($VerboseLogs) { 'info' } else { 'warn' }
    $logs = Join-Path $runData 'logs'
    New-Item -ItemType Directory -Path $logs -Force | Out-Null
    $log = Join-Path $logs ((Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
    $process = Start-Process -FilePath $binary -ArgumentList @('run', ('"' + $game + '"'), '--execute') -Wait -PassThru -WindowStyle Hidden -RedirectStandardError $log -RedirectStandardOutput ($log + '.stdout.log')
    $exitCode = $process.ExitCode
    Write-Output "Log: $log"
    if ($exitCode -ne 0) { throw "Runtime exited with code $exitCode. See $log" }
} finally {
    foreach ($entry in @(Get-ChildItem Env:VICTORY_*)) { Remove-Item -LiteralPath ('Env:' + $entry.Name) }
    foreach ($key in $old.Keys) { Set-Item -LiteralPath ('Env:' + $key) -Value $old[$key] }
}
