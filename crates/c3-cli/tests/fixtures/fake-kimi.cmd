@echo off
set "FAKE_KIMI_ARGS=%*"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-kimi.ps1"
exit /b %ERRORLEVEL%
