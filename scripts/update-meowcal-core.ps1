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
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$defaultOutputPath = Join-Path $repositoryRoot "config\meowcal-core.lock.json"
if (-not $OutputPath) {
    $OutputPath = $defaultOutputPath
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

function Get-ReleaseList {
    if ($ReleaseJsonPath) {
        if (-not (Test-Path -LiteralPath $ReleaseJsonPath -PathType Leaf)) {
            throw "Release fixture is missing: $ReleaseJsonPath"
        }
        return @(Get-Content -LiteralPath $ReleaseJsonPath -Raw | ConvertFrom-Json)
    }

    $headers = @{ Accept = "application/vnd.github+json"; "User-Agent" = "Meowcal-Core-Updater" }
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_TOKEN)) {
        $headers.Authorization = "Bearer $env:GITHUB_TOKEN"
    }
    $releases = [System.Collections.Generic.List[object]]::new()
    for ($page = 1; $page -le 100; $page++) {
        try {
            $pageReleases = @(Invoke-RestMethod `
                -Headers $headers `
                -Uri "https://api.github.com/repos/$repository/releases?per_page=100&page=$page" `
                -TimeoutSec 30)
        } catch {
            throw "Canonical Core release API request failed: $($_.Exception.Message)"
        }
        foreach ($release in $pageReleases) { [void]$releases.Add($release) }
        if ($pageReleases.Count -lt 100) { break }
    }
    return @($releases)
}

function Get-StableRelease {
    param([AllowEmptyCollection()][object[]]$Releases = @())

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

function Read-CurrentLock {
    if (-not (Test-Path -LiteralPath $OutputPath -PathType Leaf)) { return $null }
    try { $lock = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json }
    catch { throw "Existing Core lock is not valid JSON: $_" }

    $properties = @($lock.PSObject.Properties.Name)
    $required = @("schemaVersion", "repository", "tag", "coreVersion", "apiVersion", "architectures")
    if (@($required | Where-Object { $_ -notin $properties }).Count -ne 0 -or
        $lock.schemaVersion -ne 1 -or $lock.repository -ne $repository -or
        $lock.apiVersion -ne $apiVersion -or
        $lock.coreVersion -notmatch '^\d+\.\d+\.\d+$' -or
        $lock.tag -ne "core-v$($lock.coreVersion)") {
        throw "Existing Core lock has an invalid canonical identity or version."
    }
    foreach ($architecture in @("x64", "arm64")) {
        $entry = $lock.architectures.$architecture
        if ($null -eq $entry -or
            $entry.asset -ne "meowcal-core-v$($lock.coreVersion)-windows-$architecture.zip" -or
            $entry.sha256 -notmatch '^[0-9a-f]{64}$' -or
            $entry.sha256 -eq ("0" * 64)) {
            throw "Existing Core lock has an invalid $architecture identity."
        }
    }
    return $lock
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
        $binaryPath = Join-Path $temporaryDirectory "meowcal-core.exe"
        $metadataPath = Join-Path $temporaryDirectory "meowcal-core.json"
        $licensePath = Join-Path $temporaryDirectory "LICENSE"
        & (Join-Path $PSScriptRoot "verify-core-package.ps1") `
            -ArchivePath $ArchivePath `
            -ExpectedVersion $Version `
            -ExpectedApiVersion $apiVersion `
            -ExpectedArchitecture $Architecture `
            -ExpectedArchiveSha256 $ExpectedChecksum `
            -DestinationPath $binaryPath `
            -MetadataDestinationPath $metadataPath `
            -LicenseDestinationPath $licensePath | Out-Null
        if ($RunExecutableContract) {
            & (Join-Path $PSScriptRoot "test-core-executable.ps1") `
                -BinaryPath $binaryPath `
                -ExpectedVersion $Version `
                -ExpectedApiVersion $apiVersion | Out-Null
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
    New-Item -ItemType Directory -Path $AssetDirectory -Force | Out-Null
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
    $current = Read-CurrentLock
    if ($current -and [version]$current.coreVersion -gt [version]$version) {
        Write-Result -Status "unchanged" -Message "Core lock already covers $($current.coreVersion); no upgrade is needed." `
            -Version $current.coreVersion -Tag $current.tag
        return
    }
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

    if ($current -and [version]$current.coreVersion -eq [version]$version -and
        ($current.architectures.x64.sha256 -ne $x64Checksum -or
         $current.architectures.arm64.sha256 -ne $arm64Checksum)) {
        throw "Published Core tag $tag has digests different from the existing lock; refusing to rewrite an immutable pin."
    }

    $x64ArchiveHash = Test-CoreArchive -ArchivePath $assets[$x64Asset] -ExpectedChecksum $x64Checksum `
        -Version $version -Architecture x64 -RunExecutableContract:(!$SkipExecutableContractCheck)
    $arm64ArchiveHash = Test-CoreArchive -ArchivePath $assets[$arm64Asset] -ExpectedChecksum $arm64Checksum `
        -Version $version -Architecture arm64

    if ($current) {
        if ([version]$current.coreVersion -ge [version]$version) {
            Write-Result -Status "unchanged" -Message "Core lock already covers $($current.coreVersion); no upgrade is needed." `
                -Version $current.coreVersion -Tag $current.tag
            return
        }
    }

    & (Join-Path $PSScriptRoot "write-meowcal-core-lock.ps1") `
        -Version $version `
        -X64ChecksumPath $assets[$assetNames[1]] `
        -Arm64ChecksumPath $assets[$assetNames[3]] `
        -OutputPath $OutputPath | Out-Null
    Write-Result -Status "updated" -Message "Prepared exact Meowcal Core $version lock from $tag (x64 $x64ArchiveHash; arm64 $arm64ArchiveHash)." `
        -Version $version -Tag $tag -Changed $true
} finally {
    if ($removeAssetDirectory) {
        Remove-Item -LiteralPath $AssetDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
