[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$updater = Join-Path $repositoryRoot "scripts\update-meowcal-core.ps1"
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-upgrade-tests-" + [guid]::NewGuid().ToString("N")
)

function Assert-Throws {
    param([scriptblock]$Action, [string]$ExpectedSubstring)
    try { & $Action }
    catch {
        if ($_.Exception.Message -notlike "*$ExpectedSubstring*") {
            throw "Expected '$ExpectedSubstring', got '$($_.Exception.Message)'."
        }
        return
    }
    throw "Expected an error containing '$ExpectedSubstring'."
}

function New-TestPe {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Architecture)
    $bytes = [byte[]]::new(512)
    $bytes[0] = 0x4D; $bytes[1] = 0x5A
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes, 0x3C)
    [BitConverter]::GetBytes([uint32]0x00004550).CopyTo($bytes, 0x80)
    $machine = if ($Architecture -eq "arm64") { [uint16]0xAA64 } else { [uint16]0x8664 }
    [BitConverter]::GetBytes($machine).CopyTo($bytes, 0x84)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

function New-CoreAsset {
    param(
        [Parameter(Mandatory)][string]$Directory,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$Architecture,
        [int]$ApiVersion = 1
    )
    $staging = Join-Path $Directory ("staging-$Architecture")
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    try {
        $binary = Join-Path $staging "meowcal-core.exe"
        New-TestPe -Path $binary -Architecture $Architecture
        $license = Join-Path $staging "LICENSE"
        [IO.File]::WriteAllText($license, "license`n")
        $metadata = [ordered]@{
            schemaVersion = 1; coreVersion = $Version; apiVersion = $ApiVersion
            os = "windows"; architecture = $Architecture; executable = "meowcal-core.exe"
            executableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash.ToLowerInvariant()
            license = "LICENSE"
            licenseSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $license).Hash.ToLowerInvariant()
        }
        [IO.File]::WriteAllText(
            (Join-Path $staging "meowcal-core.json"),
            (($metadata | ConvertTo-Json) + "`n"),
            [Text.UTF8Encoding]::new($false)
        )
        $asset = Join-Path $Directory "meowcal-core-v$Version-windows-$Architecture.zip"
        Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $asset -Force
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $asset).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText("$asset.sha256", "$hash  $([IO.Path]::GetFileName($asset))`n")
        return $asset
    } finally { Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue }
}

function New-ReleaseFixture {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$AssetDirectory,
        [switch]$Prerelease,
        [int]$ApiVersion = 1
    )
    $x64 = New-CoreAsset -Directory $AssetDirectory -Version $Version -Architecture x64 -ApiVersion $ApiVersion
    $arm64 = New-CoreAsset -Directory $AssetDirectory -Version $Version -Architecture arm64 -ApiVersion $ApiVersion
    $assetPaths = @($x64, "$x64.sha256", $arm64, "$arm64.sha256")
    $assets = @($assetPaths | ForEach-Object {
        $name = [IO.Path]::GetFileName($_)
        [ordered]@{ name = $name; browser_download_url = "https://github.com/PeterShanxin/Meowcal-Sub/releases/download/core-v$Version/$name" }
    })
    [IO.File]::WriteAllText(
        $Path,
        ((@([ordered]@{
            tag_name = "core-v$Version"; draft = $false; prerelease = [bool]$Prerelease; assets = $assets
        }) | ConvertTo-Json -Depth 6) + "`n"),
        [Text.UTF8Encoding]::new($false)
    )
}

function Invoke-Updater {
    param(
        [Parameter(Mandatory)][string]$ReleasePath,
        [Parameter(Mandatory)][string]$AssetDirectory,
        [Parameter(Mandatory)][string]$LockPath,
        [Parameter(Mandatory)][string]$ResultPath
    )
    & $updater -ReleaseJsonPath $ReleasePath -AssetDirectory $AssetDirectory `
        -OutputPath $LockPath -ResultPath $ResultPath -SkipExecutableContractCheck | Out-Null
    return Get-Content -LiteralPath $ResultPath -Raw | ConvertFrom-Json
}

New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
try {
    $assets = Join-Path $temporaryDirectory "assets"
    $noRelease = Join-Path $temporaryDirectory "no-release.json"
    $lock = Join-Path $temporaryDirectory "lock.json"
    $result = Join-Path $temporaryDirectory "result.json"
    New-Item -ItemType Directory -Path $assets | Out-Null
    $noReleaseData = @(
        [ordered]@{ tag_name = "core-v9.9.9"; draft = $false; prerelease = $true; assets = @() },
        [ordered]@{ tag_name = "v0.7.0"; draft = $false; prerelease = $false; assets = @() }
    )
    [IO.File]::WriteAllText($noRelease, (($noReleaseData | ConvertTo-Json -Depth 5) + "`n"))
    $noReleaseResult = Invoke-Updater -ReleasePath $noRelease -AssetDirectory $assets `
        -LockPath $lock -ResultPath $result
    if ($noReleaseResult.status -ne "no-release" -or (Test-Path $lock)) {
        throw "A prerelease or application release must not create a Core lock."
    }

    $release = Join-Path $temporaryDirectory "release.json"
    New-ReleaseFixture -Path $release -Version "0.2.0" -AssetDirectory $assets
    $updated = Invoke-Updater -ReleasePath $release -AssetDirectory $assets -LockPath $lock -ResultPath $result
    if ($updated.status -ne "updated" -or -not $updated.changed) { throw "A valid stable release must update the lock." }
    $lockObject = Get-Content -LiteralPath $lock -Raw | ConvertFrom-Json
    if ($lockObject.tag -ne "core-v0.2.0" -or
        $lockObject.architectures.x64.sha256 -notmatch '^[0-9a-f]{64}$' -or
        $lockObject.architectures.arm64.sha256 -notmatch '^[0-9a-f]{64}$') {
        throw "The generated lock must contain exact release asset digests."
    }
    $lockHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $lock).Hash
    $same = Invoke-Updater -ReleasePath $release -AssetDirectory $assets -LockPath $lock -ResultPath $result
    if ($same.status -ne "unchanged" -or (Get-FileHash -Algorithm SHA256 -LiteralPath $lock).Hash -ne $lockHash) {
        throw "Repeating the same release must be idempotent."
    }

    $x64Checksum = Join-Path $assets "meowcal-core-v0.2.0-windows-x64.zip.sha256"
    [IO.File]::WriteAllText($x64Checksum, ("a" * 64) + "  meowcal-core-v0.2.0-windows-x64.zip`n")
    Assert-Throws { Invoke-Updater -ReleasePath $release -AssetDirectory $assets `
        -LockPath (Join-Path $temporaryDirectory "hash-fail.json") -ResultPath $result } "SHA-256 mismatch"

    $badApiAssets = Join-Path $temporaryDirectory "bad-api-assets"
    New-Item -ItemType Directory -Path $badApiAssets | Out-Null
    $badApiRelease = Join-Path $temporaryDirectory "bad-api.json"
    New-ReleaseFixture -Path $badApiRelease -Version "0.3.0" -AssetDirectory $badApiAssets -ApiVersion 2
    Assert-Throws { Invoke-Updater -ReleasePath $badApiRelease -AssetDirectory $badApiAssets `
        -LockPath (Join-Path $temporaryDirectory "api-fail.json") -ResultPath $result } "metadata does not match"

    $olderAssets = Join-Path $temporaryDirectory "older-assets"
    New-Item -ItemType Directory -Path $olderAssets | Out-Null
    $olderRelease = Join-Path $temporaryDirectory "older.json"
    New-ReleaseFixture -Path $olderRelease -Version "0.1.0" -AssetDirectory $olderAssets
    $olderResult = Invoke-Updater -ReleasePath $olderRelease -AssetDirectory $olderAssets `
        -LockPath $lock -ResultPath $result
    if ($olderResult.status -ne "unchanged") { throw "The updater must refuse to downgrade a newer lock." }

    $workflow = Get-Content -LiteralPath (Join-Path $repositoryRoot ".github\workflows\core-upgrade.yml") -Raw
    foreach ($requirement in @(
        'schedule:', 'workflow_dispatch:', 'CORE_UPGRADE_TOKEN',
        'peter-evans/create-pull-request@22a9089034f40e5a961c8808d113e2c98fb63676'
    )) {
        if ($workflow -notmatch [regex]::Escape($requirement)) {
            throw "Core upgrade workflow is missing $requirement."
        }
    }
    $consumerCi = Get-Content -LiteralPath (Join-Path $repositoryRoot ".github\workflows\test.yml") -Raw
    if ($consumerCi -notmatch '(?m)^\s*pull_request:\s*$') {
        throw "Consumer CI must retain a pull_request trigger for generated upgrade PRs."
    }
    if ($workflow -match 'token:\s*\$\{\{\s*github\.token') {
        throw "Core upgrade PR creation must use an explicitly configured token so pull_request CI is emitted."
    }

    Write-Host "Core upgrade automation tests passed." -ForegroundColor Green
} finally { Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue }
