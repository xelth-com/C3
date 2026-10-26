@echo off
set "FAKE_GROK_ARGS=%*"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-grok.ps1"
exit /b %ERRORLEVEL%
