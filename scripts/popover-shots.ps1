# Development check: open each of the given popovers by clicking its dock
# element through the page, take an in-page screenshot, and tile the crops
# into one image. Selectors are CSS selectors of the element to click.
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [Parameter(Mandatory = $true)][string[]]$Selectors,
  [int]$Width = 620,
  [int]$Height = 640
)

Add-Type -AssemblyName System.Drawing
$cdp = Join-Path $PSScriptRoot 'cdp.mjs'
$tiles = @()
foreach ($selector in $Selectors) {
  $center = node $cdp eval dock "(() => { const el = document.querySelector('$selector'); if (!el) return 'missing'; el.click(); const r = el.getBoundingClientRect(); return Math.round((r.left + r.width / 2) * devicePixelRatio); })()"
  if ($center -eq 'missing') { "not found: $selector"; continue }
  Start-Sleep -Milliseconds 1300
  $file = Join-Path $env:TEMP "glassdock-popover-$($tiles.Count).png"
  node $cdp shot dock $file | Out-Null
  node $cdp eval dock "(document.querySelector('$selector').click(), 1)" | Out-Null
  Start-Sleep -Milliseconds 500
  $tiles += [pscustomobject]@{ File = $file; Center = [int]$center }
}

$sheet = New-Object System.Drawing.Bitmap ($Width * [Math]::Max(1, $tiles.Count)), $Height
$g = [System.Drawing.Graphics]::FromImage($sheet)
$g.Clear([System.Drawing.Color]::FromArgb(70, 72, 80))
for ($i = 0; $i -lt $tiles.Count; $i++) {
  $img = [System.Drawing.Image]::FromFile($tiles[$i].File)
  $sx = [Math]::Max(0, [Math]::Min($img.Width - $Width, $tiles[$i].Center - $Width / 2))
  $src = New-Object System.Drawing.Rectangle $sx, ($img.Height - $Height), $Width, $Height
  $dst = New-Object System.Drawing.Rectangle ($i * $Width), 0, $Width, $Height
  $g.DrawImage($img, $dst, $src, 'Pixel')
  $img.Dispose()
  Remove-Item $tiles[$i].File -ErrorAction SilentlyContinue
}
$g.Dispose()
$sheet.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$sheet.Dispose()
"saved $Out"
