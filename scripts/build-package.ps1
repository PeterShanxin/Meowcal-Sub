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
Import-Module (Join-Path $PSScriptRoot 'package-build.psm1') -Force
$hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

if ($Architecture -eq "auto") {
    $Architecture = if ($hostArchitecture -eq "Arm64") { "arm64" } else { "x64" }
}
if ($Architecture -eq 'arm64' -and $hostArchitecture -ne 'Arm64') {
    throw 'ARM64 packaging requires an ARM64 host to execute the Core handshake.'
}

$targetTriple = if ($Architecture -eq "arm64") {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
}

if (-not $CargoTargetDir) {
    $CargoTargetDir = Join-Path ([IO.Path]::GetTempPath()) "meowcal-sub-build-$Architecture"
}
$CargoTargetDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($CargoTargetDir)
$environmentNames = @('CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_PROFILE_RELEASE_CODEGEN_UNITS',
    'CARGO_PROFILE_RELEASE_LTO', 'CARGO_PROFILE_RELEASE_STRIP', 'CARGO_INCREMENTAL', 'MEOWCAL_CORE_EXECUTABLE')
$savedEnvironment = @{}
foreach ($name in $environmentNames) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$startedAt = [DateTimeOffset]::UtcNow
$steps = [Collections.Generic.List[object]]::new()
$succeeded = $false
$bundleArgs = if ($Bundles -eq "all") { "nsis,msi" } else { $Bundles }

Push-Location $repositoryRoot
try {
    # The local Snapdragon safeguard follows the host, including x64 cross-builds.
    # Hosted packaging can override it explicitly without changing the release profile.
    if ($hostArchitecture -eq 'Arm64' -and [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_JOBS)) {
        $env:CARGO_BUILD_JOBS = '1'
    }
    if ($Architecture -eq "arm64") {
        $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"
        $env:CARGO_PROFILE_RELEASE_LTO = "false"
        $env:CARGO_PROFILE_RELEASE_STRIP = "false"
        $env:CARGO_INCREMENTAL = "0"
    }
    $env:CARGO_TARGET_DIR = $CargoTargetDir
    Write-Host "Cargo jobs: $env:CARGO_BUILD_JOBS; target: $targetTriple; output: $CargoTargetDir"

    Invoke-PackageBuildStep -Name 'Fetch pinned Core' -Steps $steps -Action {
        & (Join-Path $repositoryRoot "scripts\fetch-meowcal-core.ps1") -Architecture $Architecture
    }
    Invoke-PackageBuildStep -Name 'Build OverlayHost' -Steps $steps -Action {
        & (Join-Path $repositoryRoot "scripts\build-overlayhost.ps1") -Architecture $Architecture
        if ($LASTEXITCODE -ne 0) { throw "OverlayHost build failed for $Architecture." }
    }
    Invoke-PackageBuildStep -Name 'Core consumer handshake' -Steps $steps -Action {
        $env:MEOWCAL_CORE_EXECUTABLE = Join-Path $repositoryRoot "src-tauri\resources\core\meowcal-core.exe"
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
        $env:MEOWCAL_CORE_EXECUTABLE = $savedEnvironment['MEOWCAL_CORE_EXECUTABLE']
    }

    Invoke-PackageBuildStep -Name 'Build and sign Tauri bundles' -Steps $steps -Action {
        npx --no -- tauri build --target $targetTriple --bundles $bundleArgs -- --locked
        if ($LASTEXITCODE -ne 0) { throw "Tauri package build failed for $Architecture." }
    }

    $bundleDirectory = Join-Path $CargoTargetDir "$targetTriple\release\bundle"
    Write-Host "Package output: $bundleDirectory" -ForegroundColor Green
    Get-ChildItem -LiteralPath $bundleDirectory -Recurse -File |
        Select-Object FullName, Length
    $succeeded = $true
} finally {
    try {
        Write-PackageBuildReport -Path (Join-Path $CargoTargetDir 'package-build.json') `
            -Architecture $Architecture -StartedAt $startedAt -Succeeded $succeeded -Steps $steps
    } finally {
        Pop-Location
        foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
    }
}
