# Print the state Glassdock is supposed to manage, for checks during
# development: taskbar auto-hide and visibility, the monitor work area, the
# dock window, the restore journal and the running processes.
param([switch]$WaitForStart, [int]$TimeoutSeconds = 240)

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class GdShell {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct APPBARDATA { public uint cbSize; public IntPtr hWnd; public uint uCallbackMessage; public uint uEdge; public RECT rc; public IntPtr lParam; }
  [DllImport("shell32.dll")] public static extern UIntPtr SHAppBarMessage(uint msg, ref APPBARDATA data);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, IntPtr title);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "FindWindowW")] public static extern IntPtr FindByTitle(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowRgnBox(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint action, uint param, out RECT rect, uint winIni);
  public static uint State() { var d = new APPBARDATA(); d.cbSize = (uint)Marshal.SizeOf(typeof(APPBARDATA)); return (uint)SHAppBarMessage(4, ref d); }
}
'@
[void][GdShell]::SetProcessDPIAware()

$log = "$env:LOCALAPPDATA\Glassdock\logs\glassdock.log"
if ($WaitForStart) {
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  $since = (Get-Date).AddSeconds(-5)
  $ready = $false
  while ((Get-Date) -lt $deadline) {
    $running = Get-Process glassdock -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -or $true }
    if ($running -and (Test-Path $log) -and ((Get-Item $log).LastWriteTime -gt $since) -and
        ([GdShell]::FindByTitle([IntPtr]::Zero, 'Glassdock') -ne [IntPtr]::Zero)) { $ready = $true; break }
    Start-Sleep -Seconds 2
  }
  "app ready: $ready"
  Start-Sleep -Seconds 3
}

$tray = [GdShell]::FindWindowW('Shell_TrayWnd', [IntPtr]::Zero)
$work = New-Object GdShell+RECT
[void][GdShell]::SystemParametersInfoW(0x30, 0, [ref]$work, 0)
$state = [GdShell]::State()
"taskbar: state=$state (bit 1 = auto-hide) window visible=$([GdShell]::IsWindowVisible($tray))"
"primary work area: ($($work.L),$($work.T))-($($work.R),$($work.B))"

$dock = [GdShell]::FindByTitle([IntPtr]::Zero, 'Glassdock')
if ($dock -ne [IntPtr]::Zero) {
  $r = New-Object GdShell+RECT; [void][GdShell]::GetWindowRect($dock, [ref]$r)
  $b = New-Object GdShell+RECT; $kind = [GdShell]::GetWindowRgnBox($dock, [ref]$b)
  "dock window: visible=$([GdShell]::IsWindowVisible($dock)) rect=($($r.L),$($r.T))-($($r.R),$($r.B)) region box=($($b.L),$($b.T))-($($b.R),$($b.B)) kind=$kind foreground=$([GdShell]::GetForegroundWindow() -eq $dock)"
} else {
  'dock window: not found'
}

$journal = "$env:LOCALAPPDATA\Glassdock\taskbar-restore.json"
if (Test-Path $journal) { "journal: $((Get-Content $journal -Raw) -replace '\s+', ' ')" } else { 'journal: none' }
$procs = Get-CimInstance Win32_Process -Filter "Name='glassdock.exe'"
"glassdock processes: $(@($procs).Count) $(($procs | ForEach-Object { if ($_.CommandLine -match '--watchdog') { 'watchdog' } else { 'dock' } }) -join ', ')"
