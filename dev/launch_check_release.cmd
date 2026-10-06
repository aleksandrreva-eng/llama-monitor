@echo off
REM Launch the freshly built release exe (portable copy), let the WebView2
REM frontend mount and receive at least one monitoring:update, then dump the log.
REM The log tells us which of the two happened:
REM   - "set_options" lines present  -> UI mounted and is issuing its IPC calls
REM   - "frontend error: ..."        -> the UI crashed (the bug we are chasing)
setlocal
set EXE=F:\AI\Monitor_Ornith\llama-monitor\src-tauri\target\release\llama-monitor.exe
set LOG=F:\AI\Monitor_Ornith\llama-monitor\src-tauri\target\release\llama-monitor.log
set APPLOG=%LOCALAPPDATA%\llama-monitor\llama-monitor.log
if exist "%LOG%" del /q "%LOG%"
if exist "%APPLOG%" del /q "%APPLOG%"
start "" "%EXE%"
timeout /t 16 /nobreak >nul
taskkill /IM llama-monitor.exe /F >nul 2>&1
echo ==== LOG BESIDE EXE ====
if exist "%LOG%" (type "%LOG%") else (echo (no log beside exe))
echo ==== LOG IN LOCALAPPDATA ====
if exist "%APPLOG%" (type "%APPLOG%") else (echo (no log in LOCALAPPDATA))
