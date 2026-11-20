param(
  [Parameter(Mandatory=$true)][string]$ProcessName,
  [Parameter(Mandatory=$true)][string]$Output,
  [int]$Width = 1440,
  [int]$Height = 900,
  [switch]$Maximize
)

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WindowCapture {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern void SwitchToThisWindow(IntPtr hWnd, bool altTab);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@

$process = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } |
  Select-Object -First 1
if (-not $process) { throw "No window for process $ProcessName" }

$handle = $process.MainWindowHandle
if ($Maximize) {
  [WindowCapture]::ShowWindow($handle, 3) | Out-Null
} else {
  [WindowCapture]::ShowWindow($handle, 9) | Out-Null
  [WindowCapture]::MoveWindow($handle, 60, 40, $Width, $Height, $true) | Out-Null
}
# Bring the target to the foreground using both the shell and Win32 paths; PowerShell itself is
# usually not allowed to steal focus with SetForegroundWindow alone.
$shell = New-Object -ComObject WScript.Shell
$shell.AppActivate($process.Id) | Out-Null
[WindowCapture]::SwitchToThisWindow($handle, $true)
[WindowCapture]::SetForegroundWindow($handle) | Out-Null
if (-not $Maximize) {
  [WindowCapture]::SetWindowPos($handle, [IntPtr](-1), 60, 40, $Width, $Height, 0x0040) | Out-Null
}
Start-Sleep -Milliseconds 1500
[WindowCapture]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0003) | Out-Null
Start-Sleep -Milliseconds 400

$rect = New-Object WindowCapture+RECT
if (-not [WindowCapture]::GetWindowRect($handle, [ref]$rect)) { throw "GetWindowRect failed for $ProcessName" }
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bitmap = New-Object System.Drawing.Bitmap $w, $h
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))
$directory = Split-Path -Parent $Output
if ($directory -and -not (Test-Path $directory)) { New-Item -ItemType Directory -Force -Path $directory | Out-Null }
$bitmap.Save($Output, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Host "captured $ProcessName $($process.MainWindowTitle) -> $Output (${w}x${h})"
