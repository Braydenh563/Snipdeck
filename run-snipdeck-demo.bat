@echo off
rem Builds and starts Snipdeck on a seeded, in-memory demo library (nothing is saved).
rem Extra arguments go to the app, e.g.  run-snipdeck-demo.bat --screenshot panel.png
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust is not installed. Run run-windows-tests.bat once ^(it offers to install it^), then try again.
    exit /b 1
)
cd /d "%~dp0"
cargo run --locked -p snipdeck-app -- --demo %*
endlocal
