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

$probeDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-core-version-probe-" + [guid]::NewGuid().ToString("N")
)
try {
    New-Item -ItemType Directory -Path $probeDirectory | Out-Null
    $stdoutPath = Join-Path $probeDirectory "stdout.txt"
    $stderrPath = Join-Path $probeDirectory "stderr.txt"
    $process = Start-Process -FilePath $BinaryPath -ArgumentList "--version-json" `
        -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath `
        -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(15000)) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        throw "Core executable timed out while answering --version-json."
    }
    if ($process.ExitCode -ne 0) {
        throw "Core executable did not answer --version-json."
    }
    $versionJson = Get-Content -LiteralPath $stdoutPath -Raw
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $probeDirectory -Recurse -Force -ErrorAction SilentlyContinue
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
