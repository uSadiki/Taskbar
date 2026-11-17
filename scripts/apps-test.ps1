# Development check for the Apps widget's actions, using a File Explorer
# window opened for the purpose: launch from the pin, focus, minimize by
# clicking the active app, restore, then close. No input is generated; the
# dock's own commands are invoked through the page.
$cdp = Join-Path $PSScriptRoot 'cdp.mjs'
function Invoke-Dock($expr) { node $cdp eval dock "window.__TAURI_INTERNALS__.invoke($expr).then(r => typeof r === 'string' ? r : JSON.stringify(r ?? null))" }
function Explorer { node $cdp eval dock "window.__TAURI_INTERNALS__.invoke('apps_list').then(g => { const a = g.find(x => x.name === 'File Explorer'); return a ? JSON.stringify({ id: a.id, active: a.active, windows: a.windows.map(w => ({ min: w.minimized, active: w.active })) }) : 'null'; })" | ConvertFrom-Json }

$results = @()
$app = Explorer
if (-not $app) { Write-Error 'File Explorer is not in the dock'; exit 1 }
$id = $app.id
$before = $app.windows.Count
"File Explorer pin: $id, windows before: $before"

Invoke-Dock "'apps_activate', { id: '$id' }" | Out-Null
if ($before -gt 0) { Invoke-Dock "'apps_launch', { id: '$id' }" | Out-Null }
# A cold start of File Explorer can take several seconds.
$deadline = (Get-Date).AddSeconds(12)
do { Start-Sleep -Milliseconds 700; $app = Explorer } while ($app.windows.Count -le $before -and (Get-Date) -lt $deadline)
$results += "launch opens a window: $($app.windows.Count -gt $before)"

# Focus it (it may already be in front), then click again: the active app minimizes.
if (-not $app.active) { Invoke-Dock "'apps_activate', { id: '$id' }" | Out-Null; Start-Sleep -Milliseconds 900; $app = Explorer }
$results += "window is active after focus: $($app.active)"
Invoke-Dock "'apps_activate', { id: '$id' }" | Out-Null
Start-Sleep -Milliseconds 1200
$app = Explorer
$results += "clicking the active app minimizes it: $(@($app.windows | Where-Object { $_.min }).Count -ge 1)"
Invoke-Dock "'apps_activate', { id: '$id' }" | Out-Null
Start-Sleep -Milliseconds 1200
$app = Explorer
$results += "clicking again restores and focuses it: $($app.active -and @($app.windows | Where-Object { $_.min }).Count -eq 0)"

if ($before -eq 0) {
  Invoke-Dock "'apps_close_all', { id: '$id' }" | Out-Null
  Start-Sleep -Milliseconds 1500
  $app = Explorer
  $results += "close all closes it: $($app.windows.Count -eq 0)"
} else {
  $results += 'close all: skipped (File Explorer windows were already open and are left alone)'
}
$results
if ($results -match 'False') { 'RESULT: FAIL' } else { 'RESULT: pass' }
