@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Manage-InputFlow-Autostart.ps1" -Action Status
exit /b %errorlevel%
