@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Manage-InputFlow-Autostart.ps1" -Action Disable
exit /b %errorlevel%
