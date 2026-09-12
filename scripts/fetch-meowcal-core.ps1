[CmdletBinding()]
param(
    [ValidateSet("auto", "x64", "arm64")]
    [string]$Architecture = "auto",
    [string]$LockPath,
    [string]$ArchivePath,
    [string]$DestinationPath,
    [switch]$Offline,
    [switch]$SkipExecutableContractCheck
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
if (-not $LockPath) {
    $LockPath = if ($env:MEOWCAL_CORE_LOCK) {
        $env:MEOWCAL_CORE_LOCK
    } else {
        Join-Path $repositoryRoot "config\meowcal-core.lock.json"
    }
}
if (-not $DestinationPath) {
    $DestinationPath = Join-Path $repositoryRoot "src-tauri\resources\core\meowcal-core.exe"
}
if (-not $ArchivePath -and $env:MEOWCAL_CORE_ARCHIVE) {
    $ArchivePath = $env:MEOWCAL_CORE_ARCHIVE
}
if (-not (Test-Path -LiteralPath $LockPath -PathType Leaf)) {
    throw "Missing reviewed Core release lock: $LockPath"
}

$lock = Get-Content -LiteralPath $LockPath -Raw | ConvertFrom-Json
$expectedLockProperties = @(
    "schemaVersion", "repository", "tag", "coreVersion", "apiVersion", "architectures"
)
$lockProperties = @($lock.PSObject.Properties.Name)
if (@($lockProperties | Where-Object { $_ -notin $expectedLockProperties }).Count -ne 0 -or
    @($expectedLockProperties | Where-Object { $_ -notin $lockProperties }).Count -ne 0) {
    throw "Core release lock fields do not match schema 1."
}
if ($lock.schemaVersion -isnot [long] -or $lock.schemaVersion -ne 1 -or
    $lock.repository -ne "PeterShanxin/Meowcal-Sub" -or
    $lock.apiVersion -isnot [long] -or $lock.apiVersion -ne 1) {
    throw "Core release lock identity does not match schema 1 and API 1."
}
if ($lock.coreVersion -notmatch '^\d+\.\d+\.\d+$' -or
    $lock.tag -ne "core-v$($lock.coreVersion)") {
    throw "Core release lock version and tag do not match."
}

$lockedArchitectureNames = @($lock.architectures.PSObject.Properties.Name)
if ($lockedArchitectureNames.Count -ne 2 -or
    @($lockedArchitectureNames | Where-Object { $_ -notin @("x64", "arm64") }).Count -ne 0) {
    throw "Core release lock architectures must contain only x64 and arm64."
}
foreach ($name in @("x64", "arm64")) {
    $entry = $lock.architectures.$name
    $entryProperties = @($entry.PSObject.Properties.Name)
    $expectedAsset = "meowcal-core-v$($lock.coreVersion)-windows-$name.zip"
    if ($null -eq $entry -or $entryProperties.Count -ne 2 -or
        "asset" -notin $entryProperties -or "sha256" -notin $entryProperties -or
        $entry.asset -ne $expectedAsset) {
        throw "Core $name lock entry does not match $expectedAsset."
    }
    if ($entry.sha256 -notmatch '^[0-9a-f]{64}$' -or $entry.sha256 -eq ("0" * 64)) {
        throw "Core $name SHA-256 must be a real lowercase digest."
    }
}

$selected = $lock.architectures.$Architecture
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-fetch-" + [guid]::NewGuid().ToString("N")
)

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    if (-not $ArchivePath) {
        if ($Offline) {
            throw "Offline Core preparation requires -ArchivePath or MEOWCAL_CORE_ARCHIVE."
        }
        $ArchivePath = Join-Path $temporaryDirectory $selected.asset
        $uri = "https://github.com/$($lock.repository)/releases/download/$($lock.tag)/$($selected.asset)"
        Invoke-WebRequest -Uri $uri -OutFile $ArchivePath -TimeoutSec 120
    }
    if (-not (Test-Path -LiteralPath $ArchivePath -PathType Leaf)) {
        throw "Core archive is missing: $ArchivePath"
    }

    $destinationDirectory = Split-Path -Parent $DestinationPath
    if (-not $destinationDirectory) {
        $destinationDirectory = (Get-Location).Path
    }
    New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
    $candidate = Join-Path $destinationDirectory (
        ".meowcal-core-" + [guid]::NewGuid().ToString("N") + ".exe"
    )
    $candidateMetadata = "$candidate.json"
    $candidateLicense = "$candidate.LICENSE"
    $metadataDestination = Join-Path $destinationDirectory "meowcal-core.json"
    $licenseDestination = Join-Path $destinationDirectory "LICENSE"
    try {
        $verified = & (Join-Path $PSScriptRoot "verify-core-package.ps1") `
            -ArchivePath $ArchivePath `
            -ExpectedVersion $lock.coreVersion `
            -ExpectedApiVersion $lock.apiVersion `
            -ExpectedArchitecture $Architecture `
            -ExpectedArchiveSha256 $selected.sha256 `
            -DestinationPath $candidate `
            -MetadataDestinationPath $candidateMetadata `
            -LicenseDestinationPath $candidateLicense
        if (-not $SkipExecutableContractCheck) {
            & (Join-Path $PSScriptRoot "test-core-executable.ps1") `
                -BinaryPath $candidate `
                -ExpectedVersion $lock.coreVersion `
                -ExpectedApiVersion $lock.apiVersion | Out-Null
        }
        Move-Item -LiteralPath $candidateLicense -Destination $licenseDestination -Force
        Move-Item -LiteralPath $candidateMetadata -Destination $metadataDestination -Force
        Move-Item -LiteralPath $candidate -Destination $DestinationPath -Force
    } finally {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $candidateMetadata -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $candidateLicense -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Prepared pinned Meowcal Core $($lock.coreVersion) $Architecture at $DestinationPath."
    [pscustomobject]@{
        Version = $lock.coreVersion
        ApiVersion = $lock.apiVersion
        Architecture = $Architecture
        ArchiveSha256 = $verified.ArchiveSha256
        ExecutableSha256 = $verified.ExecutableSha256
        DestinationPath = $DestinationPath
        MetadataPath = $metadataDestination
        LicensePath = $licenseDestination
    }
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
