[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ArchivePath,
    [Parameter(Mandatory)][string]$ExpectedVersion,
    [Parameter(Mandatory)]
    [ValidateSet("x64", "arm64")]
    [string]$ExpectedArchitecture,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$ExpectedApiVersion = 1,
    [string]$ExpectedArchiveSha256,
    [string]$DestinationPath,
    [string]$MetadataDestinationPath,
    [string]$LicenseDestinationPath
)

$ErrorActionPreference = "Stop"

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

if (-not (Test-Path -LiteralPath $ArchivePath -PathType Leaf)) {
    throw "Core archive is missing: $ArchivePath"
}
if ($ExpectedVersion -notmatch '^\d+\.\d+\.\d+$') {
    throw "Expected Core version must use major.minor.patch."
}

$archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
if ($ExpectedArchiveSha256) {
    if ($ExpectedArchiveSha256 -notmatch '^[0-9a-fA-F]{64}$') {
        throw "Expected Core archive SHA-256 must contain 64 hexadecimal characters."
    }
    if ($archiveHash -ne $ExpectedArchiveSha256.ToLowerInvariant()) {
        throw "Core archive SHA-256 mismatch. Expected $ExpectedArchiveSha256, got $archiveHash."
    }
}

$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-verify-" + [guid]::NewGuid().ToString("N")
)

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $ArchivePath))
    try {
        $entries = @($zip.Entries)
        $entryNames = @($entries | ForEach-Object FullName)
        $expectedEntries = @("LICENSE", "meowcal-core.exe", "meowcal-core.json")
        $maxEntryBytes = 256MB
        $maxTotalBytes = 512MB
        [long]$totalUncompressedBytes = 0
        if ($entries.Count -ne $expectedEntries.Count -or
            @($entryNames | Where-Object { $_ -notin $expectedEntries }).Count -ne 0 -or
            @($expectedEntries | Where-Object { $_ -notin $entryNames }).Count -ne 0) {
            throw "Core archive must contain only LICENSE, meowcal-core.exe, and meowcal-core.json at its root."
        }
        foreach ($entry in $entries) {
            if ($entry.FullName -ne $entry.Name -or $entry.FullName.Contains("..")) {
                throw "Core archive contains an unsafe path: $($entry.FullName)"
            }
            if ($entry.Length -gt $maxEntryBytes) {
                throw "Core archive entry $($entry.Name) exceeds the 256 MiB extraction limit."
            }
            $totalUncompressedBytes += [long]$entry.Length
            if ($totalUncompressedBytes -gt $maxTotalBytes) {
                throw "Core archive exceeds the 512 MiB extraction limit."
            }
        }
    } finally {
        $zip.Dispose()
    }

    [IO.Compression.ZipFile]::ExtractToDirectory(
        (Resolve-Path -LiteralPath $ArchivePath),
        $temporaryDirectory
    )
    $metadataPath = Join-Path $temporaryDirectory "meowcal-core.json"
    $binaryPath = Join-Path $temporaryDirectory "meowcal-core.exe"
    $licensePath = Join-Path $temporaryDirectory "LICENSE"
    $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json

    $expectedProperties = @(
        "schemaVersion", "coreVersion", "apiVersion", "os", "architecture",
        "executable", "executableSha256", "license", "licenseSha256"
    )
    $actualProperties = @($metadata.PSObject.Properties.Name)
    if (@($actualProperties | Where-Object { $_ -notin $expectedProperties }).Count -ne 0 -or
        @($expectedProperties | Where-Object { $_ -notin $actualProperties }).Count -ne 0) {
        throw "Core package metadata fields do not match schema 1."
    }
    if ($metadata.schemaVersion -isnot [long] -or $metadata.schemaVersion -ne 1) {
        throw "Unsupported Core package metadata schema."
    }
    if ($metadata.coreVersion -ne $ExpectedVersion) {
        throw "Core package version '$($metadata.coreVersion)' does not match '$ExpectedVersion'."
    }
    if ($metadata.apiVersion -isnot [long] -or $metadata.apiVersion -ne $ExpectedApiVersion) {
        throw "Core API version '$($metadata.apiVersion)' does not match '$ExpectedApiVersion'."
    }
    if ($metadata.os -ne "windows") { throw "Core package OS must be windows." }
    if ($metadata.architecture -ne $ExpectedArchitecture) {
        throw "Core package architecture '$($metadata.architecture)' does not match '$ExpectedArchitecture'."
    }
    if ($metadata.executable -ne "meowcal-core.exe") {
        throw "Core package executable must be meowcal-core.exe."
    }
    if ($metadata.executableSha256 -notmatch '^[0-9a-f]{64}$') {
        throw "Core executable SHA-256 must be lowercase hexadecimal."
    }
    if ($metadata.license -ne "LICENSE" -or $metadata.licenseSha256 -notmatch '^[0-9a-f]{64}$') {
        throw "Core package license metadata is invalid."
    }
    $binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binaryPath).Hash.ToLowerInvariant()
    if ($binaryHash -ne $metadata.executableSha256) {
        throw "Core executable SHA-256 mismatch. Expected $($metadata.executableSha256), got $binaryHash."
    }
    $licenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $licensePath).Hash.ToLowerInvariant()
    if ($licenseHash -ne $metadata.licenseSha256) {
        throw "Core license SHA-256 mismatch. Expected $($metadata.licenseSha256), got $licenseHash."
    }
    $expectedMachine = if ($ExpectedArchitecture -eq "arm64") { 0xAA64 } else { 0x8664 }
    $actualMachine = Get-PeMachine -Path $binaryPath
    if ($actualMachine -ne $expectedMachine) {
        throw ("Core executable PE machine 0x{0:X4} does not match {1}." -f $actualMachine, $ExpectedArchitecture)
    }

    if ($DestinationPath) {
        $destinationDirectory = Split-Path -Parent $DestinationPath
        if ($destinationDirectory) {
            New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
        }
        Copy-Item -LiteralPath $binaryPath -Destination $DestinationPath -Force
    }
    if ($MetadataDestinationPath) {
        $metadataDestinationDirectory = Split-Path -Parent $MetadataDestinationPath
        if ($metadataDestinationDirectory) {
            New-Item -ItemType Directory -Path $metadataDestinationDirectory -Force | Out-Null
        }
        Copy-Item -LiteralPath $metadataPath -Destination $MetadataDestinationPath -Force
    }
    if ($LicenseDestinationPath) {
        $licenseDestinationDirectory = Split-Path -Parent $LicenseDestinationPath
        if ($licenseDestinationDirectory) {
            New-Item -ItemType Directory -Path $licenseDestinationDirectory -Force | Out-Null
        }
        Copy-Item -LiteralPath $licensePath -Destination $LicenseDestinationPath -Force
    }

    [pscustomobject]@{
        ArchivePath = (Resolve-Path -LiteralPath $ArchivePath).Path
        ArchiveSha256 = $archiveHash
        ExecutableSha256 = $binaryHash
        Version = $metadata.coreVersion
        ApiVersion = $metadata.apiVersion
        Architecture = $metadata.architecture
        DestinationPath = $DestinationPath
        MetadataDestinationPath = $MetadataDestinationPath
        LicenseDestinationPath = $LicenseDestinationPath
    }
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
