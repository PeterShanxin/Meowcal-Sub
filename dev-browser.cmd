@echo off
setlocal

REM =============================================================================
REM DEV-BROWSER.CMD - Browser Dev Mode Launcher
REM =============================================================================
REM This script starts both the HTTP backend and frontend server for testing
REM the app in a browser (useful for AI agents like Claude).
REM
REM Usage: .\dev-browser.cmd
REM =============================================================================

echo.
echo =====================================================================
echo   MEOWCAL SUB - Browser Dev Mode
echo =====================================================================
echo.
echo Starting HTTP backend and frontend servers...
echo.

REM Resolve the build directory. Browser mode builds with whatever toolchain is
REM already on PATH, so it asks for the build directory only and does not fail
REM when no Visual Studio installation can be found.
set "MEOWCAL_PS=powershell"
where pwsh >nul 2>&1 && set "MEOWCAL_PS=pwsh"

REM Cleared first, so a missing line is unambiguously a failed resolution rather
REM than a CARGO_TARGET_DIR this shell already carried.
set "MEOWCAL_RESOLVED_CARGO_TARGET_DIR="

for /f "usebackq tokens=1,* delims==" %%A in (`%MEOWCAL_PS% -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\dev-environment.ps1" -Emit CargoTargetDir`) do set "%%A=%%B"

if not defined MEOWCAL_RESOLVED_CARGO_TARGET_DIR (
    echo ERROR: could not resolve a build directory. See the message above.
    exit /b 1
)
set "CARGO_TARGET_DIR=%MEOWCAL_RESOLVED_CARGO_TARGET_DIR%"

REM Start the HTTP backend in a new window
echo [1/2] Starting HTTP backend server (port 3001)...
start "Meowcal Sub Backend" cmd /k "cd /d %~dp0 && npm run dev:backend"

REM Wait a few seconds for the backend to start
echo Waiting for backend to initialize...
timeout /t 5 /nobreak > nul

REM Start the frontend server
echo [2/2] Starting frontend server (port 3000)...
echo.
echo =====================================================================
echo   Ready! Open http://localhost:3000 in your browser
echo =====================================================================
echo.

cd /d %~dp0
call npm run dev:browser

endlocal
