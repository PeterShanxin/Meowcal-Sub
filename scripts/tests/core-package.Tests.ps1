[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$packageScript = Join-Path $repositoryRoot "scripts\package-core.ps1"
$verifyScript = Join-Path $repositoryRoot "scripts\verify-core-package.ps1"
$verifyReleaseScript = Join-Path $repositoryRoot "scripts\verify-core-release-assets.ps1"
$fetchScript = Join-Path $repositoryRoot "scripts\fetch-meowcal-core.ps1"
$writeLockScript = Join-Path $repositoryRoot "scripts\write-meowcal-core-lock.ps1"
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-package-tests-" + [guid]::NewGuid().ToString("N")
)

function Assert-Throws {
    param([scriptblock]$Action, [string]$ExpectedSubstring)

    try {
        & $Action
    } catch {
        if ($_.Exception.Message -notlike "*$ExpectedSubstring*") {
            throw "Expected an error containing '$ExpectedSubstring', got '$($_.Exception.Message)'."
        }
        return
    }
    throw "Expected an error containing '$ExpectedSubstring', but the action succeeded."
}

function New-TestPe {
    param([Parameter(Mandatory)][string]$Path, [string]$Architecture = "x64")

    $bytes = [byte[]]::new(512)
    $bytes[0] = 0x4D
    $bytes[1] = 0x5A
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes, 0x3C)
    [BitConverter]::GetBytes([uint32]0x00004550).CopyTo($bytes, 0x80)
    $machine = if ($Architecture -eq "arm64") { [uint16]0xAA64 } else { [uint16]0x8664 }
    [BitConverter]::GetBytes($machine).CopyTo($bytes, 0x84)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

function New-TestArchive {
    param(
        [Parameter(Mandatory)][string]$Path,
        [string]$Version = "0.1.0",
        [int]$ApiVersion = 1,
        [string]$Architecture = "x64",
        [string]$PeArchitecture = $Architecture,
        [switch]$CorruptExecutableHash,
        [switch]$CorruptLicenseHash,
        [switch]$ExtraFile
    )

    $staging = "$Path.staging"
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    try {
        $binary = Join-Path $staging "meowcal-core.exe"
        New-TestPe -Path $binary -Architecture $PeArchitecture
        $binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash.ToLowerInvariant()
        if ($CorruptExecutableHash) { $binaryHash = "f" * 64 }
        $licensePath = Join-Path $staging "LICENSE"
        [IO.File]::WriteAllText($licensePath, "test license`n")
        $licenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $licensePath).Hash.ToLowerInvariant()
        if ($CorruptLicenseHash) { $licenseHash = "d" * 64 }
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
        [IO.File]::WriteAllText(
            (Join-Path $staging "meowcal-core.json"),
            (($metadata | ConvertTo-Json) + "`n"),
            [Text.UTF8Encoding]::new($false)
        )
        if ($ExtraFile) {
            [IO.File]::WriteAllText((Join-Path $staging "unexpected.txt"), "no")
        }
        Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $Path -Force
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
    }
}

New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
try {
    $binary = Join-Path $temporaryDirectory "meowcal-core.exe"
    New-TestPe -Path $binary
    $output = Join-Path $temporaryDirectory "output"
    $packageOutput = @(& $packageScript `
        -Architecture x64 `
        -OutputDirectory $output `
        -BinaryPath $binary `
        -Version 0.1.0 `
        -SkipExecutableContractCheck)
    $archive = Join-Path $output "meowcal-core-v0.1.0-windows-x64.zip"
    $checksum = "$archive.sha256"
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf) -or
        -not (Test-Path -LiteralPath $checksum -PathType Leaf)) {
        throw "Packaging did not produce the versioned archive and checksum. Output: $packageOutput"
    }
    $checksumHash = ((Get-Content -LiteralPath $checksum -Raw).Trim() -split '  ')[0]
    $destination = Join-Path $temporaryDirectory "installed\meowcal-core.exe"
    $metadataDestination = Join-Path (Split-Path -Parent $destination) "meowcal-core.json"
    $licenseDestination = Join-Path (Split-Path -Parent $destination) "LICENSE"
    $verified = & $verifyScript `
        -ArchivePath $archive `
        -ExpectedVersion 0.1.0 `
        -ExpectedApiVersion 1 `
        -ExpectedArchitecture x64 `
        -ExpectedArchiveSha256 $checksumHash `
        -DestinationPath $destination `
        -MetadataDestinationPath $metadataDestination `
        -LicenseDestinationPath $licenseDestination
    if ($verified.ArchiveSha256 -ne $checksumHash) {
        throw "Verifier returned an unexpected archive digest."
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Leaf) -or
        -not (Test-Path -LiteralPath $metadataDestination -PathType Leaf) -or
        -not (Test-Path -LiteralPath $licenseDestination -PathType Leaf)) {
        throw "Verifier did not retain the validated Core resource set."
    }

    $arm64Binary = Join-Path $temporaryDirectory "meowcal-core-arm64.exe"
    New-TestPe -Path $arm64Binary -Architecture arm64
    & $packageScript `
        -Architecture arm64 `
        -OutputDirectory $output `
        -BinaryPath $arm64Binary `
        -Version 0.1.0 `
        -SkipExecutableContractCheck | Out-Null
    & $verifyReleaseScript -Directory $output -Version 0.1.0

    $lockPath = Join-Path $temporaryDirectory "meowcal-core.lock.json"
    & $writeLockScript `
        -Version 0.1.0 `
        -X64ChecksumPath $checksum `
        -Arm64ChecksumPath (Join-Path $output "meowcal-core-v0.1.0-windows-arm64.zip.sha256") `
        -OutputPath $lockPath | Out-Null
    $pinnedDestination = Join-Path $temporaryDirectory "pinned\meowcal-core.exe"
    $pinned = & $fetchScript `
        -Architecture x64 `
        -LockPath $lockPath `
        -ArchivePath $archive `
        -DestinationPath $pinnedDestination `
        -Offline `
        -SkipExecutableContractCheck
    if ($pinned.ArchiveSha256 -ne $checksumHash -or
        -not (Test-Path -LiteralPath $pinnedDestination -PathType Leaf) -or
        -not (Test-Path -LiteralPath $pinned.MetadataPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $pinned.LicensePath -PathType Leaf)) {
        throw "The product fetcher did not retain the exact locked Core resource set."
    }
    $pinnedMetadata = Get-Content -LiteralPath $pinned.MetadataPath -Raw | ConvertFrom-Json
    if ($pinnedMetadata.executableSha256 -ne
            (Get-FileHash -Algorithm SHA256 -LiteralPath $pinnedDestination).Hash.ToLowerInvariant() -or
        $pinnedMetadata.licenseSha256 -ne
            (Get-FileHash -Algorithm SHA256 -LiteralPath $pinned.LicensePath).Hash.ToLowerInvariant()) {
        throw "Fetched Core resource hashes do not match its retained metadata."
    }

    $tauriResources = (Get-Content -LiteralPath (
        Join-Path $repositoryRoot "src-tauri\tauri.conf.json"
    ) -Raw | ConvertFrom-Json).bundle.resources
    foreach ($requiredResource in @(
        "resources/core/meowcal-core.exe",
        "resources/core/meowcal-core.json",
        "resources/core/LICENSE"
    )) {
        if ($requiredResource -notin $tauriResources) {
            throw "Tauri bundle is missing Core resource: $requiredResource"
        }
    }

    $previousLock = $env:MEOWCAL_CORE_LOCK
    $previousArchive = $env:MEOWCAL_CORE_ARCHIVE
    try {
        $env:MEOWCAL_CORE_LOCK = $lockPath
        $env:MEOWCAL_CORE_ARCHIVE = $archive
        & $fetchScript -Architecture x64 -DestinationPath $pinnedDestination `
            -Offline -SkipExecutableContractCheck | Out-Null
    } finally {
        $env:MEOWCAL_CORE_LOCK = $previousLock
        $env:MEOWCAL_CORE_ARCHIVE = $previousArchive
    }

    $wrongLock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
    $wrongLock.architectures.x64.sha256 = "b" * 64
    $wrongLockPath = Join-Path $temporaryDirectory "wrong-lock.json"
    [IO.File]::WriteAllText(
        $wrongLockPath,
        (($wrongLock | ConvertTo-Json -Depth 5) + "`n"),
        [Text.UTF8Encoding]::new($false)
    )
    Assert-Throws {
        & $fetchScript -Architecture x64 -LockPath $wrongLockPath `
            -ArchivePath $archive -DestinationPath $pinnedDestination `
            -Offline -SkipExecutableContractCheck
    } "archive SHA-256 mismatch"

    $packageBuildSource = Get-Content -LiteralPath (Join-Path $repositoryRoot "scripts\build-package.ps1") -Raw
    if ($packageBuildSource -notmatch 'fetch-meowcal-core\.ps1' -or
        $packageBuildSource -match 'prepare-core-resource\.ps1' -or
        $packageBuildSource -match 'SkipExecutableContractCheck' -or
        $packageBuildSource -notmatch 'real_core_handshake_status_and_shutdown' -or
        $packageBuildSource -notmatch '1 passed; 0 failed;') {
        throw "Product packaging must consume the pinned Core release instead of rebuilding Core source."
    }

    Assert-Throws {
        & $verifyScript -ArchivePath $archive -ExpectedVersion 9.9.9 `
            -ExpectedArchitecture x64 -ExpectedArchiveSha256 $checksumHash
    } "does not match"
    Assert-Throws {
        & $verifyScript -ArchivePath $archive -ExpectedVersion 0.1.0 `
            -ExpectedApiVersion 2 -ExpectedArchitecture x64 -ExpectedArchiveSha256 $checksumHash
    } "API version"
    Assert-Throws {
        & $verifyScript -ArchivePath $archive -ExpectedVersion 0.1.0 `
            -ExpectedArchitecture arm64 -ExpectedArchiveSha256 $checksumHash
    } "architecture"
    Assert-Throws {
        & $verifyScript -ArchivePath $archive -ExpectedVersion 0.1.0 `
            -ExpectedArchitecture x64 -ExpectedArchiveSha256 ("0" * 64)
    } "archive SHA-256 mismatch"

    $corrupt = Join-Path $temporaryDirectory "corrupt.zip"
    New-TestArchive -Path $corrupt -CorruptExecutableHash
    Assert-Throws {
        & $verifyScript -ArchivePath $corrupt -ExpectedVersion 0.1.0 -ExpectedArchitecture x64
    } "executable SHA-256 mismatch"

    $corruptLicense = Join-Path $temporaryDirectory "corrupt-license.zip"
    New-TestArchive -Path $corruptLicense -CorruptLicenseHash
    Assert-Throws {
        & $verifyScript -ArchivePath $corruptLicense -ExpectedVersion 0.1.0 -ExpectedArchitecture x64
    } "license SHA-256 mismatch"

    $extra = Join-Path $temporaryDirectory "extra.zip"
    New-TestArchive -Path $extra -ExtraFile
    Assert-Throws {
        & $verifyScript -ArchivePath $extra -ExpectedVersion 0.1.0 -ExpectedArchitecture x64
    } "must contain only"

    $swappedMachine = Join-Path $temporaryDirectory "swapped-machine.zip"
    New-TestArchive -Path $swappedMachine -PeArchitecture arm64
    Assert-Throws {
        & $verifyScript -ArchivePath $swappedMachine -ExpectedVersion 0.1.0 -ExpectedArchitecture x64
    } "PE machine"

    Assert-Throws {
        & $packageScript -Architecture x64 -OutputDirectory $output `
            -BinaryPath $binary -Version 9.9.9 -SkipExecutableContractCheck
    } "does not match"

    Write-Host "Core package contract tests passed." -ForegroundColor Green
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
