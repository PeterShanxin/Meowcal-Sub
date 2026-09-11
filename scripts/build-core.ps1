[CmdletBinding()]
param(
    [ValidateSet("auto", "x64", "arm64")]
    [string]$Architecture = "auto",
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$CargoTargetDir
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "core\Cargo.toml"

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Missing standalone Core manifest: $manifestPath"
}

if ($Architecture -eq "auto") {
    $Architecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "Arm64" { "arm64" }
        "X64" { "x64" }
        default { throw "Meowcal Core supports only Windows x64 and ARM64 hosts." }
    }
}

$hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Architecture -eq "arm64" -and $hostArchitecture -ne "Arm64") {
    throw "An ARM64 Core executable must be built on an ARM64 host so its package handshake can run."
}

$targetTriple = if ($Architecture -eq "arm64") {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
}

if (-not $CargoTargetDir) {
    $CargoTargetDir = Join-Path $repositoryRoot "core\target"
}

$hostIsArm64 = $hostArchitecture -eq "Arm64"
if ($hostIsArm64 -and [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_JOBS)) {
    $env:CARGO_BUILD_JOBS = "1"
}

$arguments = @(
    "build",
    "--manifest-path", $manifestPath,
    "--locked",
    "--target", $targetTriple,
    "--target-dir", $CargoTargetDir,
    "--bin", "meowcal-core"
)
if ($Configuration -eq "Release") {
    $arguments += "--release"
}

& cargo @arguments
if ($LASTEXITCODE -ne 0) {
    throw "Meowcal Core $Configuration build failed for $Architecture."
}

$profile = $Configuration.ToLowerInvariant()
$binaryPath = Join-Path $CargoTargetDir "$targetTriple\$profile\meowcal-core.exe"
if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Core build succeeded without producing $binaryPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw
$versionMatch = [regex]::Match(
    $manifest,
    '(?ms)^\[package\].*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
)
if (-not $versionMatch.Success) {
    throw "core/Cargo.toml must declare a major.minor.patch package version."
}
& (Join-Path $PSScriptRoot "test-core-executable.ps1") `
    -BinaryPath $binaryPath `
    -ExpectedVersion $versionMatch.Groups["version"].Value | Out-Null

Write-Output $binaryPath
