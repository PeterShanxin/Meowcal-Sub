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

# rustc is a host-architecture process whatever it targets, so the ARM64
# compiler-stack limit follows the host, not the package. Serializing rustc is
# enough on its own to cross-build x64 from an ARM64 host - measured in
# docs/evidence/2026-08-07-arm64-host-cross-builds-x64-package.json - and it
# changes build time only, never the emitted code.
# Read the machine, not the process: PROCESSOR_ARCHITECTURE reports AMD64 to an
# emulated x64 shell on an ARM64 machine.
$hostIsArm64 =
    [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64"
if ($hostIsArm64) {
    $env:CARGO_BUILD_JOBS = "1"
}

# The rest relaxes the release profile itself and therefore changes the binary
# that ships. It stays tied to the ARM64 *target*, which is where it was proven,
# so an x64 package is optimized identically no matter which host built it.
if ($Architecture -eq "arm64") {
    $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"
    $env:CARGO_PROFILE_RELEASE_LTO = "false"
    $env:CARGO_PROFILE_RELEASE_STRIP = "false"
    $env:CARGO_INCREMENTAL = "0"
}

$env:CARGO_TARGET_DIR = $CargoTargetDir
$bundleArgs = if ($Bundles -eq "all") { "nsis,msi" } else { $Bundles }

Push-Location $repositoryRoot
try {
    # A PowerShell script reports failure by throwing; it never sets
    # $LASTEXITCODE, which is still null here in a fresh CI session.
    & (Join-Path $repositoryRoot "scripts\fetch-meowcal-core.ps1") `
        -Architecture $Architecture

    & (Join-Path $repositoryRoot "scripts\build-overlayhost.ps1") -Architecture $Architecture
    if ($LASTEXITCODE -ne 0) {
        throw "OverlayHost build failed for $Architecture."
    }

    $coreExecutable = Join-Path $repositoryRoot "src-tauri\resources\core\meowcal-core.exe"
    $previousCoreExecutable = $env:MEOWCAL_CORE_EXECUTABLE
    try {
        $env:MEOWCAL_CORE_EXECUTABLE = $coreExecutable
        $handshakeOutput = @(cargo test `
            --manifest-path src-tauri\Cargo.toml `
            --locked `
            --target $targetTriple `
            --target-dir $CargoTargetDir `
            --lib `
            core_client::tests::real_core_handshake_status_and_shutdown `
            -- --ignored --exact 2>&1)
        $handshakeExitCode = $LASTEXITCODE
        $handshakeOutput | ForEach-Object { Write-Host $_ }
        $handshakeText = $handshakeOutput -join "`n"
        if ($handshakeExitCode -ne 0 -or
            $handshakeText -notmatch 'test result: ok\. 1 passed; 0 failed;') {
            throw "The packaged Core failed the compiled Sub 1 handshake."
        }
    } finally {
        $env:MEOWCAL_CORE_EXECUTABLE = $previousCoreExecutable
    }

    # The pinned CLI from package.json, never whatever npx would fetch. A CLI
    # older than the `tauri` crate cannot patch `__TAURI_BUNDLE_TYPE` into the
    # binary, and the updater needs that byte to know how it was installed.
    npx --no -- tauri build --target $targetTriple --bundles $bundleArgs
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
