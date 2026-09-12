[CmdletBinding()]
param(
    [string]$ReleaseJsonPath,
    [string]$AssetDirectory,
    [string]$OutputPath,
    [string]$ResultPath,
    [switch]$SkipExecutableContractCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = "PeterShanxin/Meowcal-Sub"
$apiVersion = 1
$requiredCapabilities = @(
    "status", "install", "ready", "complete", "shutdown", "ocrInitialize",
    "ocrLanguages", "ocrRecognizeBgra"
)
$repositoryRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputPath) {
    $OutputPath = Join-Path $repositoryRoot "config\meowcal-core.lock.json"
}

function Write-Result {
    param(
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)][string]$Message,
        [string]$Version,
        [string]$Tag,
        [bool]$Changed = $false
    )

    $result = [ordered]@{
        status = $Status
        message = $Message
        changed = $Changed
        coreVersion = $Version
        tag = $Tag
        outputPath = $OutputPath
    }
    if ($ResultPath) {
        $resultDirectory = Split-Path -Parent $ResultPath
        if ($resultDirectory) { New-Item -ItemType Directory -Path $resultDirectory -Force | Out-Null }
        [IO.File]::WriteAllText(
            $ResultPath,
            (($result | ConvertTo-Json -Depth 5) + "`n"),
            [Text.UTF8Encoding]::new($false)
        )
    }
    Write-Host $Message
}

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        try {
            if ($stream.Length -lt 0x40 -or $reader.ReadUInt16() -ne 0x5A4D) {
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

function Get-ReleaseList {
    if ($ReleaseJsonPath) {
        if (-not (Test-Path -LiteralPath $ReleaseJsonPath -PathType Leaf)) {
            throw "Release fixture is missing: $ReleaseJsonPath"
        }
        return @(Get-Content -LiteralPath $ReleaseJsonPath -Raw | ConvertFrom-Json)
    }

    try {
        return @(Invoke-RestMethod `
            -Headers @{ Accept = "application/vnd.github+json"; "User-Agent" = "Meowcal-Core-Updater" } `
            -Uri "https://api.github.com/repos/$repository/releases?per_page=100" `
            -TimeoutSec 30)
    } catch {
        throw "Canonical Core release API request failed: $($_.Exception.Message)"
    }
}

function Get-StableRelease {
    param([Parameter(Mandatory)][object[]]$Releases)

    $candidates = foreach ($release in $Releases) {
        if ($release.draft -or $release.prerelease) { continue }
        $match = [regex]::Match([string]$release.tag_name, '^core-v(?<version>\d+\.\d+\.\d+)$')
        if (-not $match.Success) { continue }
        [pscustomobject]@{
            Release = $release
            Version = $match.Groups["version"].Value
            SortVersion = [version]$match.Groups["version"].Value
        }
    }
    return $candidates | Sort-Object SortVersion -Descending | Select-Object -First 1
}

function Get-AssetFile {
    param(
        [Parameter(Mandatory)][object]$Asset,
        [Parameter(Mandatory)][string]$AssetName,
        [Parameter(Mandatory)][string]$Directory,
        [Parameter(Mandatory)][string]$Tag
    )

    $expectedUrl = "https://github.com/$repository/releases/download/$Tag/$AssetName"
    if ([string]$Asset.browser_download_url -ne $expectedUrl) {
        throw "Core release asset $AssetName does not use the canonical GitHub download URL."
    }
    $path = Join-Path $Directory $AssetName
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        if (-not $Asset.browser_download_url) {
            throw "Core release asset $AssetName has no download URL."
        }
        try {
            Invoke-WebRequest -Headers @{ "User-Agent" = "Meowcal-Core-Updater" } `
                -Uri $Asset.browser_download_url -OutFile $path -TimeoutSec 120
        } catch {
            throw "Core release asset download failed for ${AssetName}: $($_.Exception.Message)"
        }
    }
    return (Resolve-Path -LiteralPath $path).Path
}

function Get-Checksum {
    param(
        [Parameter(Mandatory)][string]$ChecksumPath,
        [Parameter(Mandatory)][string]$AssetName
    )

    $line = (Get-Content -LiteralPath $ChecksumPath -Raw).Trim()
    $match = [regex]::Match($line, "^(?<hash>[0-9a-f]{64})  $([regex]::Escape($AssetName))$")
    if (-not $match.Success -or $match.Groups["hash"].Value -eq ("0" * 64)) {
        throw "Core checksum for $AssetName must bind a real lowercase SHA-256 to the exact asset."
    }
    return $match.Groups["hash"].Value
}

function Test-CoreArchive {
    param(
        [Parameter(Mandatory)][string]$ArchivePath,
        [Parameter(Mandatory)][string]$ExpectedChecksum,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][ValidateSet("x64", "arm64")][string]$Architecture,
        [switch]$RunExecutableContract
    )

    $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
    if ($archiveHash -ne $ExpectedChecksum) {
        throw "Core $Architecture archive SHA-256 mismatch. Expected $ExpectedChecksum, got $archiveHash."
    }
    $temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
        "meowcal-core-upgrade-" + [guid]::NewGuid().ToString("N")
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
            if ($entries.Count -ne 3 -or
                @($entryNames | Where-Object { $_ -notin $expectedEntries }).Count -ne 0 -or
                @($expectedEntries | Where-Object { $_ -notin $entryNames }).Count -ne 0) {
                throw "Core $Architecture archive must contain only LICENSE, meowcal-core.exe, and meowcal-core.json at its root."
            }
            foreach ($entry in $entries) {
                if ($entry.FullName -ne $entry.Name -or $entry.FullName.Contains("..")) {
                    throw "Core $Architecture archive contains an unsafe path: $($entry.FullName)"
                }
            }
        } finally { $zip.Dispose() }

        [IO.Compression.ZipFile]::ExtractToDirectory(
            (Resolve-Path -LiteralPath $ArchivePath), $temporaryDirectory
        )
        $metadataPath = Join-Path $temporaryDirectory "meowcal-core.json"
        $binaryPath = Join-Path $temporaryDirectory "meowcal-core.exe"
        $licensePath = Join-Path $temporaryDirectory "LICENSE"
        try { $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json }
        catch { throw "Core $Architecture metadata is not valid JSON: $_" }
        $expectedProperties = @(
            "schemaVersion", "coreVersion", "apiVersion", "os", "architecture",
            "executable", "executableSha256", "license", "licenseSha256"
        )
        $actualProperties = @($metadata.PSObject.Properties.Name)
        if (@($actualProperties | Where-Object { $_ -notin $expectedProperties }).Count -ne 0 -or
            @($expectedProperties | Where-Object { $_ -notin $actualProperties }).Count -ne 0) {
            throw "Core $Architecture metadata fields do not match schema 1."
        }
        if ($metadata.schemaVersion -ne 1 -or $metadata.coreVersion -ne $Version -or
            $metadata.apiVersion -ne $apiVersion -or $metadata.os -ne "windows" -or
            $metadata.architecture -ne $Architecture -or $metadata.executable -ne "meowcal-core.exe" -or
            $metadata.license -ne "LICENSE") {
            throw "Core $Architecture package metadata does not match the release contract."
        }
        if ($metadata.executableSha256 -notmatch '^[0-9a-f]{64}$' -or
            $metadata.licenseSha256 -notmatch '^[0-9a-f]{64}$') {
            throw "Core $Architecture package metadata contains an invalid digest."
        }
        $binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binaryPath).Hash.ToLowerInvariant()
        if ($binaryHash -ne $metadata.executableSha256) {
            throw "Core $Architecture executable SHA-256 does not match package metadata."
        }
        $licenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $licensePath).Hash.ToLowerInvariant()
        if ($licenseHash -ne $metadata.licenseSha256) {
            throw "Core $Architecture license SHA-256 does not match package metadata."
        }
        $expectedMachine = if ($Architecture -eq "arm64") { 0xAA64 } else { 0x8664 }
        if ((Get-PeMachine -Path $binaryPath) -ne $expectedMachine) {
            throw "Core $Architecture executable PE machine does not match the package architecture."
        }
        if ($RunExecutableContract) {
            $versionJson = & $binaryPath --version-json 2>$null
            if ($LASTEXITCODE -ne 0) { throw "Core x64 executable did not answer --version-json." }
            try { $versionInfo = $versionJson | ConvertFrom-Json }
            catch { throw "Core x64 executable returned invalid --version-json output: $_" }
            if ($versionInfo.version -ne $Version -or $versionInfo.api -ne $apiVersion -or
                @($requiredCapabilities | Where-Object { $_ -notin $versionInfo.capabilities }).Count -ne 0) {
                throw "Core x64 executable version, API, or capabilities do not match the v1 consumer contract."
            }
        }
        return $archiveHash
    } finally {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not $AssetDirectory) {
    $AssetDirectory = Join-Path ([IO.Path]::GetTempPath()) (
        "meowcal-core-upgrade-assets-" + [guid]::NewGuid().ToString("N")
    )
    $removeAssetDirectory = $true
} else {
    New-Item -ItemType Directory -Path $AssetDirectory -Force | Out-Null
    $removeAssetDirectory = $false
}

try {
    $stable = Get-StableRelease -Releases (Get-ReleaseList)
    if (-not $stable) {
        Write-Result -Status "no-release" -Message "No published stable core-vX.Y.Z release is available."
        return
    }
    $release = $stable.Release
    $version = $stable.Version
    $tag = [string]$release.tag_name
    $assetNames = @(
        "meowcal-core-v$version-windows-x64.zip",
        "meowcal-core-v$version-windows-x64.zip.sha256",
        "meowcal-core-v$version-windows-arm64.zip",
        "meowcal-core-v$version-windows-arm64.zip.sha256"
    )
    $assets = @{}
    foreach ($assetName in $assetNames) {
        $matches = @($release.assets | Where-Object { $_.name -eq $assetName })
        if ($matches.Count -ne 1) { throw "Core release $tag must publish exactly one $assetName asset." }
        $assets[$assetName] = Get-AssetFile -Asset $matches[0] -AssetName $assetName `
            -Directory $AssetDirectory -Tag $tag
    }
    $x64Asset = $assetNames[0]
    $arm64Asset = $assetNames[2]
    $x64Checksum = Get-Checksum -ChecksumPath $assets[$assetNames[1]] -AssetName $x64Asset
    $arm64Checksum = Get-Checksum -ChecksumPath $assets[$assetNames[3]] -AssetName $arm64Asset
    $x64ArchiveHash = Test-CoreArchive -ArchivePath $assets[$x64Asset] -ExpectedChecksum $x64Checksum `
        -Version $version -Architecture x64 -RunExecutableContract:(!$SkipExecutableContractCheck)
    $arm64ArchiveHash = Test-CoreArchive -ArchivePath $assets[$arm64Asset] -ExpectedChecksum $arm64Checksum `
        -Version $version -Architecture arm64

    $current = $null
    if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
        try { $current = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json }
        catch { throw "Existing Core lock is not valid JSON: $_" }
        if ($current.repository -ne $repository -or $current.coreVersion -notmatch '^\d+\.\d+\.\d+$') {
            throw "Existing Core lock has an invalid canonical identity or version."
        }
        if ([version]$current.coreVersion -eq [version]$version -and
            ($current.architectures.x64.sha256 -ne $x64Checksum -or
             $current.architectures.arm64.sha256 -ne $arm64Checksum)) {
            throw "Published Core tag $tag has digests different from the existing lock; refusing to rewrite an immutable pin."
        }
        if ([version]$current.coreVersion -ge [version]$version) {
            Write-Result -Status "unchanged" -Message "Core lock already covers $($current.coreVersion); no upgrade is needed." `
                -Version $current.coreVersion -Tag $current.tag
            return
        }
    }

    $lock = [ordered]@{
        schemaVersion = 1
        repository = $repository
        tag = $tag
        coreVersion = $version
        apiVersion = $apiVersion
        architectures = [ordered]@{
            x64 = [ordered]@{ asset = $x64Asset; sha256 = $x64Checksum }
            arm64 = [ordered]@{ asset = $arm64Asset; sha256 = $arm64Checksum }
        }
    }
    $outputDirectory = Split-Path -Parent $OutputPath
    if ($outputDirectory) { New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null }
    [IO.File]::WriteAllText(
        $OutputPath,
        (($lock | ConvertTo-Json -Depth 5) + "`n"),
        [Text.UTF8Encoding]::new($false)
    )
    Write-Result -Status "updated" -Message "Prepared exact Meowcal Core $version lock from $tag (x64 $x64ArchiveHash; arm64 $arm64ArchiveHash)." `
        -Version $version -Tag $tag -Changed $true
} finally {
    if ($removeAssetDirectory) {
        Remove-Item -LiteralPath $AssetDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
