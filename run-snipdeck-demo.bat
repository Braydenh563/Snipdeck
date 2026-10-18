@echo off
rem Builds and starts Snipdeck on a seeded, in-memory demo library (nothing is saved).
rem Extra arguments go to the app, e.g.  run-snipdeck-demo.bat --screenshot panel.png
rem This is a DEBUG build on purpose: release builds need the production licence key.
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust is not installed. Run run-windows-tests.bat once ^(it offers to install it^), then try again.
    pause
    exit /b 1
)
cd /d "%~dp0"
echo Building Snipdeck ^(the first build takes several minutes^)...
cargo run --locked -p snipdeck-app -- --demo %*
set "CODE=%ERRORLEVEL%"
echo.
if not "%CODE%"=="0" echo Snipdeck stopped with exit code %CODE%. If this is a build error, run "git pull" and try again; otherwise send the text above.
pause
endlocal & exit /b %CODE%
