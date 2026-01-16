@echo off
setlocal
set "CARGO_TARGET_DIR=D:\cargo-build"
if not exist "D:\cargo-build" mkdir "D:\cargo-build"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=arm64 -host_arch=arm64
pushd "%~dp0"
npx tauri dev
popd
