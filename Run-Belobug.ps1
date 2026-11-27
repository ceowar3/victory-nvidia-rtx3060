param([Parameter(Mandatory)][string]$GameExe)
$ErrorActionPreference='Stop'
if(Get-Process victory -ErrorAction SilentlyContinue){throw 'Cierra la instancia actual antes de iniciar Belobug.'}
foreach($e in @(Get-ChildItem Env:VICTORY_*)){Remove-Item -LiteralPath ('Env:'+$e.Name)}
$env:VICTORY_GAME_DIR=(Split-Path -Parent (Resolve-Path -LiteralPath $GameExe).Path)
$env:VICTORY_CACHE_DIR=(Join-Path $env:LOCALAPPDATA 'VIctory-Belobug-SeriesS\cache')
$env:VICTORY_DATA_DIR=(Join-Path $env:LOCALAPPDATA 'VIctory-Belobug-SeriesS\data')
$settings=@{
VICTORY_TEXTURE_NO_WAIT='1'; VICTORY_RELAY='2'; VICTORY_WIDE_BACK='1'; VICTORY_CARD_REHASH_RUNS='1'
VICTORY_JOIN_COPIES='1'; VICTORY_INDIRECT_WINDOW='1'; VICTORY_RELAY_COPY='1'; VICTORY_CARD_STEPS='2'
VICTORY_INDIRECT_BATCH='2'; VICTORY_MAP_THREADS='1'; VICTORY_TITLE_PRIORITIES='0'; VICTORY_CARD_AWAKE='1'
VICTORY_SHOW_AFTER='1'; VICTORY_AMBIENT='0.8'; VICTORY_D3D12_DRED='light'
VICTORY_THIN='1'; VICTORY_JOIN_PARTS='8'; VICTORY_UPLOAD_AHEAD='1'; VICTORY_LOG='warn'
}
foreach($k in $settings.Keys){Set-Item ('Env:'+$k) $settings[$k]}
$logDir=Join-Path $env:LOCALAPPDATA "VIctory-Belobug-SeriesS\logs"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$stamp=Get-Date -Format yyyyMMdd-HHmmss
$p=Start-Process -FilePath (Join-Path $PSScriptRoot 'victory.exe') -ArgumentList @('run',('"' + (Resolve-Path -LiteralPath $GameExe).Path + '"'),'--console','series-s','--execute') -WorkingDirectory $PSScriptRoot -RedirectStandardOutput (Join-Path $logDir "belobug-$stamp.stdout.log") -RedirectStandardError (Join-Path $logDir "belobug-$stamp.stderr.log") -PassThru
"Belobug PID=$($p.Id); log=belobug-series-s-$stamp"

