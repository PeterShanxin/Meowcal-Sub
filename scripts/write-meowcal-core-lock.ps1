[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$X64ChecksumPath,
    [Parameter(Mandatory)][string]$Arm64ChecksumPath,
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputPath) {
    $OutputPath = Join-Path $repositoryRoot "config\meowcal-core.lock.json"
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Core version must use major.minor.patch."
}

function Read-ReleaseChecksum {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Asset)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Core release checksum is missing: $Path"
    }
    $line = (Get-Content -LiteralPath $Path -Raw).Trim()
    $match = [regex]::Match($line, "^(?<hash>[0-9a-f]{64})  $([regex]::Escape($Asset))$")
    if (-not $match.Success -or $match.Groups["hash"].Value -eq ("0" * 64)) {
        throw "Core release checksum must bind a real lowercase SHA-256 to $Asset."
    }
    return $match.Groups["hash"].Value
}

$x64Asset = "meowcal-core-v$Version-windows-x64.zip"
$arm64Asset = "meowcal-core-v$Version-windows-arm64.zip"
$lock = [ordered]@{
    schemaVersion = 1
    repository = "PeterShanxin/Meowcal-Sub"
    tag = "core-v$Version"
    coreVersion = $Version
    apiVersion = 1
    architectures = [ordered]@{
        x64 = [ordered]@{
            asset = $x64Asset
            sha256 = Read-ReleaseChecksum -Path $X64ChecksumPath -Asset $x64Asset
        }
        arm64 = [ordered]@{
            asset = $arm64Asset
            sha256 = Read-ReleaseChecksum -Path $Arm64ChecksumPath -Asset $arm64Asset
        }
    }
}
$outputDirectory = Split-Path -Parent $OutputPath
if ($outputDirectory) {
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
}
[IO.File]::WriteAllText(
    $OutputPath,
    (($lock | ConvertTo-Json -Depth 5) + "`n"),
    [Text.UTF8Encoding]::new($false)
)
Write-Host "Wrote reviewed Core release lock: $OutputPath"
Write-Output $OutputPath
