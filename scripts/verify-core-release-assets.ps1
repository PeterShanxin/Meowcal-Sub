[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Version,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$ApiVersion = 1
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
    throw "Core release asset directory is missing: $Directory"
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Core release version must use major.minor.patch."
}

$files = @(Get-ChildItem -LiteralPath $Directory -Recurse -File)
$expectedNames = @()
foreach ($architecture in @("x64", "arm64")) {
    $archiveName = "meowcal-core-v$Version-windows-$architecture.zip"
    $checksumName = "$archiveName.sha256"
    $expectedNames += $archiveName, $checksumName

    $archive = @($files | Where-Object Name -eq $archiveName)
    $checksum = @($files | Where-Object Name -eq $checksumName)
    if ($archive.Count -ne 1) {
        throw "Expected exactly one $archiveName, found $($archive.Count)."
    }
    if ($checksum.Count -ne 1) {
        throw "Expected exactly one $checksumName, found $($checksum.Count)."
    }

    $expectedHash = (Get-Content -LiteralPath $checksum[0].FullName -Raw).Trim()
    $checksumMatch = [regex]::Match(
        $expectedHash,
        "^(?<hash>[0-9a-f]{64})  $([regex]::Escape($archiveName))$"
    )
    if (-not $checksumMatch.Success) {
        throw "$checksumName must contain the lowercase SHA-256 and exact archive name."
    }

    & (Join-Path $PSScriptRoot "verify-core-package.ps1") `
        -ArchivePath $archive[0].FullName `
        -ExpectedVersion $Version `
        -ExpectedApiVersion $ApiVersion `
        -ExpectedArchitecture $architecture `
        -ExpectedArchiveSha256 $checksumMatch.Groups["hash"].Value | Out-Null
}

$unexpected = @($files | Where-Object Name -notin $expectedNames)
if ($unexpected.Count -ne 0) {
    throw "Unexpected Core release assets: $(($unexpected.Name | Sort-Object) -join ', ')"
}

Write-Host "Verified Core $Version release assets for x64 and ARM64." -ForegroundColor Green
