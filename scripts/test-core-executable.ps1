[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BinaryPath,
    [Parameter(Mandatory)][string]$ExpectedVersion,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$ExpectedApiVersion = 1
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    throw "Core executable is missing: $BinaryPath"
}

$versionJson = & $BinaryPath --version-json
if ($LASTEXITCODE -ne 0) {
    throw "Core executable did not answer --version-json."
}
try {
    $versionInfo = $versionJson | ConvertFrom-Json
} catch {
    throw "Core executable returned invalid --version-json output: $_"
}
$requiredCapabilities = @(
    "status", "install", "ready", "complete", "shutdown", "ocrInitialize",
    "ocrLanguages", "ocrRecognizeBgra"
)
if ($versionInfo.version -isnot [string] -or
    $versionInfo.version -ne $ExpectedVersion -or
    $versionInfo.api -isnot [long] -or
    $versionInfo.api -ne $ExpectedApiVersion -or
    @($requiredCapabilities | Where-Object { $_ -notin $versionInfo.capabilities }).Count -ne 0) {
    throw "Core executable version, API, or capabilities do not match the v1 package contract."
}

[pscustomobject]@{
    Version = $versionInfo.version
    ApiVersion = $versionInfo.api
    Capabilities = @($versionInfo.capabilities)
}
