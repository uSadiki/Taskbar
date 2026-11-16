# Development check for popover dismissal: with a popover open, make the dock
# the foreground window, then activate the window that was in front before
# (what a click on another window does) and report whether the popover closed.
# No input is generated.
param([string]$Open = "document.querySelector('.dock-clock').click()",
      [string]$IsOpen = "document.querySelector('.dock-clock').getAttribute('aria-expanded')")

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class GdFocus {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(IntPtr cls, string title);
  public static bool Activate(IntPtr h) {
    uint me = GetCurrentThreadId();
    uint fg = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero);
    bool attached = fg != 0 && fg != me && AttachThreadInput(me, fg, true);
    BringWindowToTop(h);
    bool ok = SetForegroundWindow(h);
    if (attached) AttachThreadInput(me, fg, false);
    return ok;
  }
}
'@
$cdp = Join-Path $PSScriptRoot 'cdp.mjs'
$dock = [GdFocus]::FindWindowW([IntPtr]::Zero, 'Glassdock')
if ($dock -eq [IntPtr]::Zero) { Write-Error 'dock window not found'; exit 1 }
$other = [GdFocus]::GetForegroundWindow()

if ((node $cdp eval dock $IsOpen) -ne 'true') { node $cdp eval dock "($Open, 'opened')" | Out-Null; Start-Sleep -Milliseconds 600 }
"popover open: $(node $cdp eval dock $IsOpen)"
[void][GdFocus]::Activate($dock)
Start-Sleep -Milliseconds 400
"dock is foreground: $([GdFocus]::GetForegroundWindow() -eq $dock); popover still open: $(node $cdp eval dock $IsOpen)"
[void][GdFocus]::Activate($other)
Start-Sleep -Milliseconds 600
$stillOpen = node $cdp eval dock $IsOpen
"after activating another window, popover open: $stillOpen"
if ($stillOpen -eq 'true') { 'RESULT: FAIL' } else { 'RESULT: pass' }
