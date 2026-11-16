# Development check: switch through every theme with the calendar open,
# capture the dock and popover for each, and tile the captures into one image.
param([Parameter(Mandatory = $true)][string]$Out, [int]$X = 1100, [int]$Y = 1060, [int]$Width = 700, [int]$Height = 540)

Add-Type -AssemblyName System.Drawing
$cdp = Join-Path $PSScriptRoot 'cdp.mjs'
$capture = Join-Path $PSScriptRoot 'capture.ps1'
$themes = 'glass', 'ios', 'white', 'black', 'midnight', 'graphite'
$original = node $cdp eval dock "window.__TAURI_INTERNALS__.invoke('get_settings').then(s => s.appearance.theme)"

if ((node $cdp eval dock "document.querySelector('.dock-clock').getAttribute('aria-expanded')") -ne 'true') {
  node $cdp eval dock "(document.querySelector('.dock-clock').click(), 1)" | Out-Null
  Start-Sleep -Milliseconds 900
}

$shots = @()
foreach ($theme in $themes) {
  node $cdp eval dock "window.__TAURI_INTERNALS__.invoke('update_settings', {patch:{appearance:{theme:'$theme'}}}).then(() => 1)" | Out-Null
  Start-Sleep -Milliseconds 700
  $file = Join-Path $env:TEMP "glassdock-theme-$theme.png"
  powershell -NoProfile -ExecutionPolicy Bypass -File $capture -Out $file -X $X -Y $Y -Width $Width -Height $Height | Out-Null
  $shots += $file
}
node $cdp eval dock "window.__TAURI_INTERNALS__.invoke('update_settings', {patch:{appearance:{theme:'$original'}}}).then(() => 1)" | Out-Null
node $cdp eval dock "(document.querySelector('.dock-clock').click(), 1)" | Out-Null

# 3 x 2 grid at 70% size, with the theme name on each tile.
$scale = 0.7
$tw = [int]($Width * $scale); $th = [int]($Height * $scale)
$sheet = New-Object System.Drawing.Bitmap ($tw * 3), ($th * 2)
$g = [System.Drawing.Graphics]::FromImage($sheet)
$g.InterpolationMode = 'HighQualityBicubic'
$font = New-Object System.Drawing.Font('Segoe UI', 13, [System.Drawing.FontStyle]::Bold)
for ($i = 0; $i -lt $shots.Count; $i++) {
  $img = [System.Drawing.Image]::FromFile($shots[$i])
  $dx = ($i % 3) * $tw; $dy = [math]::Floor($i / 3) * $th
  $g.DrawImage($img, $dx, $dy, $tw, $th)
  $img.Dispose()
  $g.FillRectangle([System.Drawing.Brushes]::Black, $dx, $dy, 110, 26)
  $g.DrawString($themes[$i], $font, [System.Drawing.Brushes]::White, $dx + 6, $dy + 2)
  Remove-Item $shots[$i] -ErrorAction SilentlyContinue
}
$g.Dispose()
$sheet.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$sheet.Dispose()
"saved $Out"
