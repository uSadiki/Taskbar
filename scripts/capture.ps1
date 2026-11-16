# Capture a rectangle of the screen (physical pixels) to a PNG, for visual
# checks of the composited dock during development.
#
#   powershell -File scripts\capture.ps1 -Out shot.png -X 0 -Y 1400 -Width 2560 -Height 200
#
# With -Dock, captures the strip around the dock window instead of a fixed
# rectangle: -Above and -Below pixels around the visible region.
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [int]$X = 0,
  [int]$Y = 0,
  [int]$Width = 0,
  [int]$Height = 0,
  [switch]$Dock,
  [int]$Above = 40,
  [int]$Below = 20
)

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class GdCapture {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowRgnBox(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
}
'@
[void][GdCapture]::SetProcessDPIAware()

if ($Dock) {
  $hwnd = [GdCapture]::FindWindowW([IntPtr]::Zero, 'Glassdock')
  if ($hwnd -eq [IntPtr]::Zero) { Write-Error 'dock window not found'; exit 1 }
  $win = New-Object GdCapture+RECT
  [void][GdCapture]::GetWindowRect($hwnd, [ref]$win)
  $box = New-Object GdCapture+RECT
  if ([GdCapture]::GetWindowRgnBox($hwnd, [ref]$box) -eq 0) {
    $box.L = 0; $box.T = 0; $box.R = $win.R - $win.L; $box.B = $win.B - $win.T
  }
  $X = $win.L
  $Width = $win.R - $win.L
  $Y = [Math]::Max(0, $win.T + $box.T - $Above)
  $Height = ($win.T + $box.B + $Below) - $Y
  Write-Output "dock window ($($win.L),$($win.T))-($($win.R),$($win.B)), region box ($($box.L),$($box.T))-($($box.R),$($box.B))"
}

if ($Width -le 0) { $Width = [GdCapture]::GetSystemMetrics(0) }
if ($Height -le 0) { $Height = [GdCapture]::GetSystemMetrics(1) }
$screenHeight = [GdCapture]::GetSystemMetrics(1)
if ($Y + $Height -gt $screenHeight) { $Height = $screenHeight - $Y }

$bitmap = New-Object System.Drawing.Bitmap $Width, $Height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($X, $Y, 0, 0, $bitmap.Size)
$graphics.Dispose()
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()
Write-Output "saved $Out ($Width x $Height at $X,$Y)"
