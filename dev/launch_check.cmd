@echo off
REM Launch the installed widget and wait, so the session stays alive long
REM enough for the WebView2 frontend to mount and emit its startup IPC calls.
set LOG=%LOCALAPPDATA%\llama-monitor\llama-monitor.log
if exist "%LOG%" del /q "%LOG%"
start "" /D "C:\Program Files\llama-monitor" "C:\Program Files\llama-monitor\llama-monitor.exe"
timeout /t 18 /nobreak >nul
echo ==== LOG ====
type "%LOG%"
