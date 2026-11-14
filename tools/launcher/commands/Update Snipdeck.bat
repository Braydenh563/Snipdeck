@echo off
rem Downloads the newest release and keeps your library.
rem Thin wrapper: runs "Snipdeck.bat update" from the Snipdeck folder three levels up
rem (this file lives in tools\launcher\commands, in the zip and in a checkout alike).
setlocal
set "BAT=%~dp0..\..\..\Snipdeck.bat"
if not exist "%BAT%" (
  echo Snipdeck.bat was not found. Keep this file inside the Snipdeck folder ^(tools\launcher\commands^).
  pause
  exit /b 1
)
if not exist "%~dp0..\snipdeck.ps1" (
  echo Snipdeck.bat was not found. Keep this file inside the Snipdeck folder ^(tools\launcher\commands^).
  pause
  exit /b 1
)
rem Double-clicked? Then Snipdeck.bat keeps the window open after information commands.
rem The command line can hold special characters from the folder name: keep it inside quotes (quotes dropped) and test by substitution.
set "CL=%cmdcmdline:"=%"
call set "R=%%CL:%~nx0=%%"
if not "%R%"=="%CL%" set "SNIPDECK_DBLCLICK=1"
call "%BAT%" update %*
endlocal & exit /b %ERRORLEVEL%
