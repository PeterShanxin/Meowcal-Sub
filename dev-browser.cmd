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

REM Set custom target dir to avoid OneDrive file locking issues
set "CARGO_TARGET_DIR=D:\cargo-build"
if not exist "%CARGO_TARGET_DIR%" mkdir "%CARGO_TARGET_DIR%"

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
