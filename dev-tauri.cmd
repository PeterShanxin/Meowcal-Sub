@echo off
setlocal

REM =============================================================================
REM DEV-TAURI.CMD - Tauri Development Launcher
REM =============================================================================
REM This script builds OverlayHost for the current architecture and then
REM starts the Tauri development server.
REM =============================================================================

REM Resolve the build directory, the Visual Studio installation, and the host
REM architecture. A batch file cannot discover any of those, so
REM scripts\dev-environment.ps1 decides them and prints KEY=VALUE lines; see that
REM script for the rules and for the override variables it honours.
set "MEOWCAL_PS=powershell"
where pwsh >nul 2>&1 && set "MEOWCAL_PS=pwsh"

REM Cleared first, so a missing line is unambiguously a failed resolution. The
REM helper writes MEOWCAL_RESOLVED_* rather than the variables these become,
REM because MEOWCAL_VSDEVCMD is an override a developer may already have
REM exported - an inherited value would otherwise survive a failed run and be
REM read here as success.
set "MEOWCAL_RESOLVED_VSDEVCMD="
set "MEOWCAL_RESOLVED_HOST_ARCH="

for /f "usebackq tokens=1,* delims==" %%A in (`%MEOWCAL_PS% -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\dev-environment.ps1"`) do set "%%A=%%B"

if not defined MEOWCAL_RESOLVED_VSDEVCMD (
    echo ERROR: could not resolve the developer environment. See the message above.
    exit /b 1
)
if not defined MEOWCAL_RESOLVED_HOST_ARCH (
    echo ERROR: could not resolve the host architecture. See the message above.
    exit /b 1
)
if not defined MEOWCAL_RESOLVED_CARGO_TARGET_DIR (
    echo ERROR: could not resolve a build directory. See the message above.
    exit /b 1
)
set "CARGO_TARGET_DIR=%MEOWCAL_RESOLVED_CARGO_TARGET_DIR%"

REM Initialize the Visual Studio environment for this host's architecture.
call "%MEOWCAL_RESOLVED_VSDEVCMD%" -arch=%MEOWCAL_RESOLVED_HOST_ARCH% -host_arch=%MEOWCAL_RESOLVED_HOST_ARCH%
if %ERRORLEVEL% neq 0 (
    echo ERROR: VsDevCmd failed to initialize the MSVC environment.
    exit /b 1
)

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
