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
$process = $null
try {
    New-Item -ItemType Directory -Path $probeDirectory | Out-Null
    $stdoutPath = Join-Path $probeDirectory "stdout.txt"
    $stderrPath = Join-Path $probeDirectory "stderr.txt"
    $stdinPath = Join-Path $probeDirectory "stdin.txt"
    [IO.File]::WriteAllText($stdinPath, '')
    $process = Start-Process -FilePath $BinaryPath -ArgumentList "--version-json" `
        -RedirectStandardInput $stdinPath -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath `
        -Environment @{ GITHUB_TOKEN = ''; GH_TOKEN = ''; CORE_UPGRADE_TOKEN = '' } `
        -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(15000)) {
        throw "Core executable timed out while answering --version-json."
    }
    if ($process.ExitCode -ne 0) {
        throw "Core executable did not answer --version-json."
    }
    $versionJson = Get-Content -LiteralPath $stdoutPath -Raw
} finally {
    if ($null -ne $process) {
        try {
            if (-not $process.HasExited) { $process.Kill($true) }
            if (-not $process.WaitForExit(5000)) {
                throw "Core executable did not exit after the version probe."
            }
        } finally { $process.Dispose() }
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
