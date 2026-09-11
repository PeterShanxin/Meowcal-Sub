[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "arm64")]
    [string]$Architecture,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$BinaryPath,
    [string]$CargoTargetDir,
    [string]$Version,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$ApiVersion = 1,
    [switch]$SkipExecutableContractCheck
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "core\Cargo.toml"

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Missing standalone Core manifest: $manifestPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw
$manifestVersionMatch = [regex]::Match(
    $manifest,
    '(?ms)^\[package\].*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
)
if (-not $manifestVersionMatch.Success) {
    throw "core/Cargo.toml must declare a major.minor.patch package version."
}
$manifestVersion = $manifestVersionMatch.Groups["version"].Value
if (-not $Version) {
    $Version = $manifestVersion
} elseif ($Version -ne $manifestVersion) {
    throw "Requested Core version $Version does not match core/Cargo.toml $manifestVersion."
}

if (-not $BinaryPath) {
    $buildArguments = @{
        Architecture = $Architecture
        Configuration = "Release"
    }
    if ($CargoTargetDir) {
        $buildArguments.CargoTargetDir = $CargoTargetDir
    }
    $BinaryPath = & (Join-Path $PSScriptRoot "build-core.ps1") @buildArguments |
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

if (-not $SkipExecutableContractCheck) {
    & (Join-Path $PSScriptRoot "test-core-executable.ps1") `
        -BinaryPath $BinaryPath `
        -ExpectedVersion $Version `
        -ExpectedApiVersion $ApiVersion | Out-Null
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$assetName = "meowcal-core-v$Version-windows-$Architecture.zip"
$archivePath = Join-Path $OutputDirectory $assetName
$checksumPath = "$archivePath.sha256"
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-package-" + [guid]::NewGuid().ToString("N")
)

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $stagedBinary = Join-Path $temporaryDirectory "meowcal-core.exe"
    Copy-Item -LiteralPath $BinaryPath -Destination $stagedBinary
    $binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedBinary).Hash.ToLowerInvariant()
    $licensePath = Join-Path $repositoryRoot "LICENSE"
    if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
        throw "Missing Core distribution license: $licensePath"
    }
    $stagedLicense = Join-Path $temporaryDirectory "LICENSE"
    Copy-Item -LiteralPath $licensePath -Destination $stagedLicense
    $licenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedLicense).Hash.ToLowerInvariant()

    $metadata = [ordered]@{
        schemaVersion = 1
        coreVersion = $Version
        apiVersion = $ApiVersion
        os = "windows"
        architecture = $Architecture
        executable = "meowcal-core.exe"
        executableSha256 = $binaryHash
        license = "LICENSE"
        licenseSha256 = $licenseHash
    }
    $metadataPath = Join-Path $temporaryDirectory "meowcal-core.json"
    $metadataJson = ($metadata | ConvertTo-Json -Depth 3) + "`n"
    [IO.File]::WriteAllText($metadataPath, $metadataJson, [Text.UTF8Encoding]::new($false))

    Remove-Item -LiteralPath $archivePath -Force -ErrorAction SilentlyContinue
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $stream = [IO.File]::Open($archivePath, [IO.FileMode]::CreateNew)
    try {
        $archive = [IO.Compression.ZipArchive]::new(
            $stream,
            [IO.Compression.ZipArchiveMode]::Create,
            $false
        )
        try {
            foreach ($entryName in @("LICENSE", "meowcal-core.exe", "meowcal-core.json")) {
                $source = Join-Path $temporaryDirectory $entryName
                $entry = $archive.CreateEntry($entryName, [IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
                $entryStream = $entry.Open()
                try {
                    $sourceStream = [IO.File]::OpenRead($source)
                    try { $sourceStream.CopyTo($entryStream) } finally { $sourceStream.Dispose() }
                } finally {
                    $entryStream.Dispose()
                }
            }
        } finally {
            $archive.Dispose()
        }
    } finally {
        $stream.Dispose()
    }

    $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText(
        $checksumPath,
        "$archiveHash  $assetName`n",
        [Text.UTF8Encoding]::new($false)
    )

    Write-Host "Core package: $archivePath" -ForegroundColor Green
    Write-Host "Core checksum: $checksumPath" -ForegroundColor Green
    Write-Output $archivePath
    Write-Output $checksumPath
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
