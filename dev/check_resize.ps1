# check_resize.ps1 - launch the widget, resize its window a few times, then dump
# the app log. Resizing is the event that used to crash the frontend:
# App.svelte's onResized handler read `payload.size.width`, but Tauri v2 passes a
# PhysicalSize ({width, height}) straight to the callback, so `payload.size` was
# undefined -> "Cannot read properties of undefined (reading 'width')".
#
# Usage:  powershell -NoProfile -ExecutionPolicy Bypass -File check_resize.ps1 [-Exe <path>]
# Always writes its findings to -Report (default: check_resize_report.txt next to
# this script), because the host that runs it does not always surface stdout.
param(
  [string]$Exe = "F:\AI\Monitor_Ornith\llama-monitor\src-tauri\target\release\llama-monitor.exe",
  [string]$Report = ""
)

$ErrorActionPreference = "Stop"
$appLog = Join-Path $env:LOCALAPPDATA "llama-monitor\llama-monitor.log"
$exeLog = [System.IO.Path]::ChangeExtension($Exe, ".log")
if ($Report -eq "") { $Report = Join-Path $PSScriptRoot "check_resize_report.txt" }
$out = New-Object System.Collections.Generic.List[string]
function Say([string]$line) { $out.Add($line); Write-Host $line }
function Finish([string]$verdict) {
  $out.Add("==== RESULT ====")
  $out.Add($verdict)
  Set-Content -Path $Report -Value $out -Encoding UTF8
  Write-Host "report: $Report"
}
# Make failures visible: without this an exception aborts the script and leaves
# no trace at all, because the host does not surface stdout.
trap {
  $out.Add("EXCEPTION: " + $_.Exception.GetType().FullName + ": " + $_.Exception.Message)
  $out.Add("AT: " + $_.InvocationInfo.PositionMessage)
  Set-Content -Path $Report -Value $out -Encoding UTF8
  break
}

if (-not (Test-Path $Exe)) { Say "EXE NOT FOUND: $Exe"; Finish "FAIL (exe missing)"; return }

Get-Process llama-monitor -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
# NOTE: the app log lives in %LOCALAPPDATA% (outside the project) and cannot be
# deleted from here, so remember where the old content ends and report only the
# lines the run under test appended.
$logOffset = 0
if (Test-Path $appLog) { $logOffset = @(Get-Content $appLog).Count }
if (Test-Path $exeLog) { Remove-Item $exeLog -Force -ErrorAction SilentlyContinue }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

Start-Process -FilePath $Exe | Out-Null
Start-Sleep -Seconds 15

$proc = Get-Process llama-monitor -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Say "PROCESS NOT RUNNING after launch"; Finish "FAIL (no process)"; return }

$hwnd = $proc.MainWindowHandle
Say "exe    : $Exe"
Say "pid    : $($proc.Id)  hwnd=$hwnd"
if ($hwnd -eq [IntPtr]::Zero) { Say "WARNING: no main window handle" }

$rect = New-Object Win+RECT
[void][Win]::GetWindowRect($hwnd, [ref]$rect)
$left = $rect.Left
$top = $rect.Top
Say "initial: ${left},${top} $($rect.Right - $rect.Left)x$($rect.Bottom - $rect.Top)"

$sizes = @(@(420, 560), @(360, 480), @(500, 620), @(320, 440))
foreach ($s in $sizes) {
  [void][Win]::MoveWindow($hwnd, $left, $top, $s[0], $s[1], $true)
  Start-Sleep -Milliseconds 800
}
Start-Sleep -Seconds 3

Say "==== APP LOG (new lines only) ===="
$new = @()
if (Test-Path $appLog) {
  $all = @(Get-Content $appLog)
  if ($all.Count -gt $logOffset) { $new = $all[$logOffset..($all.Count - 1)] }
} else { Say "(no app log written)" }
foreach ($l in $new) { Say $l }
if ($new.Count -eq 0) { Say "(nothing appended)" }

$errCount = @($new | Where-Object { $_ -match "frontend error" }).Count

Get-Process llama-monitor -ErrorAction SilentlyContinue | Stop-Process -Force

if ($errCount -gt 0) { Finish "FAIL: $errCount frontend error line(s) after resize" }
else { Finish "PASS: 0 frontend errors after 4 resizes" }
