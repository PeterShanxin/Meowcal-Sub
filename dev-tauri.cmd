@echo off
setlocal

REM =============================================================================
REM DEV-TAURI.CMD - Tauri Development Launcher
REM =============================================================================
REM This script builds OverlayHost for the current architecture and then
REM starts the Tauri development server.
REM =============================================================================

REM Set custom target dir to avoid OneDrive file locking issues
set "CARGO_TARGET_DIR=D:\cargo-build"
if not exist "D:\cargo-build" mkdir "D:\cargo-build"

REM Initialize Visual Studio environment
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=arm64 -host_arch=arm64

REM Re-add user CLI shim locations after VsDevCmd, which can reset PATH.
set "PATH=%LOCALAPPDATA%\Microsoft\WindowsApps;%USERPROFILE%\.cargo\bin;%PATH%"

pushd "%~dp0"

REM Build OverlayHost for current architecture before starting Tauri
echo Building OverlayHost for current architecture...
powershell -ExecutionPolicy Bypass -File scripts\build-overlayhost.ps1 -Architecture auto
if %ERRORLEVEL% neq 0 (
    echo ERROR: OverlayHost build failed
    popd
    exit /b 1
)

echo.
echo Starting Tauri development server...
npx tauri dev

popd
endlocal
