@echo off
set "FAKE_MNT_ROLE=codex"
set "FAKE_MNT_ARGS=%*"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-mnt.ps1"
exit /b %ERRORLEVEL%
