$ErrorActionPreference = "Stop"
$msi = "F:\AI\Monitor_Ornith\llama-monitor\src-tauri\target\release\bundle\msi\llama-monitor_0.1.0_x64_en-US.msi"
$inst = "C:\Program Files\llama-monitor\llama-monitor.exe"
$report = "F:\AI\Monitor_Ornith\llama-monitor\scripts\_install_report.txt"
$out = New-Object System.Collections.Generic.List[string]
function Say([string]$l) { $out.Add($l); Write-Host $l }
trap {
  $out.Add("EXCEPTION: " + $_.Exception.GetType().FullName + ": " + $_.Exception.Message)
  Set-Content -Path $report -Value $out -Encoding UTF8
  break
}

Get-Process llama-monitor -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

$before = (Get-Item $inst -ErrorAction SilentlyContinue).LastWriteTime
Say "installed exe before: $before"

Say "msiexec /x (uninstall)..."
Start-Process msiexec -ArgumentList '/x', "`"$msi`"", '/qn', '/norestart' -Verb RunAs -Wait
Start-Sleep -Seconds 2
$removed = -not (Test-Path $inst)
Say "removed after uninstall: $removed"

Say "msiexec /i (install)..."
Start-Process msiexec -ArgumentList '/i', "`"$msi`"", '/qn', '/norestart' -Verb RunAs -Wait
Start-Sleep -Seconds 2

$after = (Get-Item $inst -ErrorAction SilentlyContinue).LastWriteTime
$afterLen = (Get-Item $inst -ErrorAction SilentlyContinue).Length
Say "installed exe after : $after"
Say "installed exe size  : $afterLen"
Say "reinstalled (mtime changed): $($after -ne $before)"

Set-Content -Path $report -Value $out -Encoding UTF8
Say "report: $report"
