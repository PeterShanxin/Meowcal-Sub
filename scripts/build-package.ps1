[CmdletBinding()]
param(
    [ValidateSet("auto", "x64", "arm64")]
    [string]$Architecture = "auto",
    [ValidateSet("nsis", "msi", "all")]
    [string]$Bundles = "all",
    [string]$CargoTargetDir
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

if ($Architecture -eq "auto") {
    $Architecture = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
}

$targetTriple = if ($Architecture -eq "arm64") {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
}

if (-not $CargoTargetDir) {
    $CargoTargetDir = Join-Path ([IO.Path]::GetTempPath()) "meowcal-sub-build-$Architecture"
}

# ARM64 rustc can exhaust its compiler stack while compiling the release graph
# with the default parallel/LTO/strip combination. These settings are the
# smallest configuration proven to produce the ARM64 installer on this repo.
if ($Architecture -eq "arm64") {
    $env:CARGO_BUILD_JOBS = "1"
    $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"
    $env:CARGO_PROFILE_RELEASE_LTO = "false"
    $env:CARGO_PROFILE_RELEASE_STRIP = "false"
    $env:CARGO_INCREMENTAL = "0"
}

$env:CARGO_TARGET_DIR = $CargoTargetDir
$bundleArgs = if ($Bundles -eq "all") { "nsis,msi" } else { $Bundles }

Push-Location $repositoryRoot
try {
    & (Join-Path $repositoryRoot "scripts\build-overlayhost.ps1") -Architecture $Architecture
    if ($LASTEXITCODE -ne 0) {
        throw "OverlayHost build failed for $Architecture."
    }

    npx --yes @tauri-apps/cli@2 build --target $targetTriple --bundles $bundleArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri package build failed for $Architecture."
    }

    $bundleDirectory = Join-Path $CargoTargetDir "$targetTriple\release\bundle"
    Write-Host "Package output: $bundleDirectory" -ForegroundColor Green
    Get-ChildItem -LiteralPath $bundleDirectory -Recurse -File |
        Select-Object FullName, Length
} finally {
    Pop-Location
}
