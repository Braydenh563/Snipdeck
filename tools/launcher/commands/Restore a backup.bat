@echo off
rem Restores a backup you choose.
rem Thin wrapper: runs "Snipdeck.bat restore" from the Snipdeck folder three levels up
rem (this file lives in tools\launcher\commands, in the zip and in a checkout alike).
setlocal
set "BAT=%~dp0..\..\..\Snipdeck.bat"
if not exist "%BAT%" (
  echo Snipdeck.bat was not found. Keep this file inside the Snipdeck folder ^(tools\launcher\commands^).
  pause
  exit /b 1
)
rem Double-clicked? Then Snipdeck.bat keeps the window open after information commands.
echo %cmdcmdline% | find /i "%~nx0" >nul && set "SNIPDECK_DBLCLICK=1"
call "%BAT%" restore %*
endlocal & exit /b %ERRORLEVEL%
