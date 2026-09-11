[CmdletBinding()]
param(
    [ValidateSet("auto", "x64", "arm64")]
    [string]$Architecture = "auto",
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$CargoTargetDir,
    [string]$BinaryPath
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

if ($Architecture -eq "auto") {
    $Architecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "Arm64" { "arm64" }
        "X64" { "x64" }
        default { throw "Meowcal Core supports only Windows x64 and ARM64 hosts." }
    }
}

if (-not $BinaryPath) {
    $arguments = @{
        Architecture = $Architecture
        Configuration = $Configuration
    }
    if ($CargoTargetDir) {
        $arguments.CargoTargetDir = $CargoTargetDir
    }
    $BinaryPath = & (Join-Path $PSScriptRoot "build-core.ps1") @arguments |
        Select-Object -Last 1
}

if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    throw "Core executable is missing: $BinaryPath"
}

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        try {
            if ($reader.ReadUInt16() -ne 0x5A4D -or $stream.Length -lt 0x40) {
                throw "Core executable is not a PE file."
            }
            $stream.Position = 0x3C
            $peOffset = $reader.ReadUInt32()
            if ($peOffset + 6 -gt $stream.Length) { throw "Core PE header is truncated." }
            $stream.Position = $peOffset
            if ($reader.ReadUInt32() -ne 0x00004550) { throw "Core PE signature is invalid." }
            return $reader.ReadUInt16()
        } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}

$expectedMachine = if ($Architecture -eq "arm64") { 0xAA64 } else { 0x8664 }
$actualMachine = Get-PeMachine -Path $BinaryPath
if ($actualMachine -ne $expectedMachine) {
    throw ("Core executable PE machine 0x{0:X4} does not match {1}." -f $actualMachine, $Architecture)
}

$manifest = Get-Content -LiteralPath (Join-Path $repositoryRoot "core\Cargo.toml") -Raw
$versionMatch = [regex]::Match(
    $manifest,
    '(?ms)^\[package\].*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
)
if (-not $versionMatch.Success) {
    throw "core/Cargo.toml must declare a major.minor.patch package version."
}
$version = $versionMatch.Groups["version"].Value
& (Join-Path $PSScriptRoot "test-core-executable.ps1") `
    -BinaryPath $BinaryPath -ExpectedVersion $version -ExpectedApiVersion 1 | Out-Null

$resourceDirectory = Join-Path $repositoryRoot "src-tauri\resources\core"
$destination = Join-Path $resourceDirectory "meowcal-core.exe"
$metadataPath = Join-Path $resourceDirectory "meowcal-core.json"
$licensePath = Join-Path $resourceDirectory "LICENSE"
$sourceLicense = Join-Path $repositoryRoot "LICENSE"
if (-not (Test-Path -LiteralPath $sourceLicense -PathType Leaf)) {
    throw "Missing Core distribution license: $sourceLicense"
}
New-Item -ItemType Directory -Path $resourceDirectory -Force | Out-Null
Copy-Item -LiteralPath $BinaryPath -Destination $destination -Force
Copy-Item -LiteralPath $sourceLicense -Destination $licensePath -Force
$metadata = [ordered]@{
    schemaVersion = 1
    coreVersion = $version
    apiVersion = 1
    os = "windows"
    architecture = $Architecture
    executable = "meowcal-core.exe"
    executableSha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    license = "LICENSE"
    licenseSha256 = (Get-FileHash -LiteralPath $licensePath -Algorithm SHA256).Hash.ToLowerInvariant()
}
[IO.File]::WriteAllText(
    $metadataPath,
    (($metadata | ConvertTo-Json -Depth 3) + "`n"),
    [Text.UTF8Encoding]::new($false)
)
Write-Host "Prepared Tauri Core resources: $resourceDirectory"
Write-Output $destination
