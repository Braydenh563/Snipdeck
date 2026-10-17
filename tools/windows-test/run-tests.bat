@echo off
setlocal
cd /d "%~dp0"
where powershell >nul 2>nul
if errorlevel 1 (
  echo Windows PowerShell was not found on this PC.
  pause
  exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-tests.ps1" %*
echo.
pause
