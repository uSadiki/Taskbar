# Development check for the fullscreen rule: shows a blank borderless window
# covering the primary monitor for two seconds, makes it the foreground
# window, and reports whether the dock window hid and came back.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class GdFullscreen {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(IntPtr cls, string title);
  // Borrow the foreground thread's input state so SetForegroundWindow is
  // allowed without generating any input.
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
[void][GdFullscreen]::SetProcessDPIAware()
$dock = [GdFullscreen]::FindWindowW([IntPtr]::Zero, 'Glassdock')
if ($dock -eq [IntPtr]::Zero) { Write-Error 'dock window not found'; exit 1 }
$previous = [GdFullscreen]::GetForegroundWindow()

$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.ShowInTaskbar = $false
$form.Bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$form.BackColor = [System.Drawing.Color]::FromArgb(24, 26, 32)
$label = New-Object System.Windows.Forms.Label
$label.Text = 'Glassdock fullscreen test - closes in 2 seconds'
$label.ForeColor = 'White'
$label.AutoSize = $true
$label.Font = New-Object System.Drawing.Font('Segoe UI', 16)
$label.Location = New-Object System.Drawing.Point(60, 60)
$form.Controls.Add($label)
$form.Show()
[void][GdFullscreen]::Activate($form.Handle)

$during = @()
$end = (Get-Date).AddMilliseconds(2000)
while ((Get-Date) -lt $end) {
  [System.Windows.Forms.Application]::DoEvents()
  Start-Sleep -Milliseconds 100
  $during += [GdFullscreen]::IsWindowVisible($dock)
}
$form.Close()
$form.Dispose()
if ($previous -ne [IntPtr]::Zero) { [void][GdFullscreen]::Activate($previous) }
Start-Sleep -Milliseconds 1500

$summary = ($during | Group-Object | ForEach-Object { "$($_.Name) x$($_.Count)" }) -join ', '
"during fullscreen, dock visible: $summary"
"after fullscreen, dock visible: $([GdFullscreen]::IsWindowVisible($dock))"
$hidLastSample = -not $during[-1]
if ($hidLastSample -and [GdFullscreen]::IsWindowVisible($dock)) { 'RESULT: pass' } else { 'RESULT: FAIL' }
