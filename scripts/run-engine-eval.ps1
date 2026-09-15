<#
.SYNOPSIS
Runs one production-pipeline subtitle evaluation against historical config A,
B, or C.

.DESCRIPTION
Config A uses the app-managed baseline. Config B is an isolated runtime
control. Config C is an isolated candidate. Use PlanOnly to inspect the exact
cargo and server arguments without reading configs, writing results, or
starting a process.

.EXAMPLE
.\scripts\run-engine-eval.ps1 -Config a -AppConfigPath "$env:APPDATA\com.meowcal.sub\config.json" -Dataset .\evals\subtitle-eval-v1.json

.EXAMPLE
.\scripts\run-engine-eval.ps1 -Config c -RuntimePath "D:\llama runtime\llama-server.exe" -ModelPath "D:\models\candidate model.gguf" -ModelAlias "candidate model" -ServerPort 11652 -AppConfigPath "$env:APPDATA\com.meowcal.sub\config.json" -Dataset .\evals\subtitle-eval-v1.json -Seed 2026 -PlanOnly
#>
# Historical config meanings:
#   A: app-managed HY-MT1.5 baseline.
#   B: isolated HY-MT1.5 control on a newer runtime.
#   C: isolated candidate model on the same runtime as B.
#
# Isolated configs require explicit runtime and model paths. Model aliases,
# server settings, dataset, app config, and output locations are parameters so
# the runner can be reused without machine-specific defaults.
# =============================================================================

[CmdletBinding()]
param(
    [ValidateSet("a", "b", "c")]
    [string]$Config = "",
    [ValidateSet("mt15", "mt2")]
    [string]$Engine = "",
    [ValidateRange(1, 20)]
    [int]$Runs = 1,
    [string]$ReportPath,
    [string]$ResultsDir,
    [string]$AppConfigPath,
    [string]$Dataset,
    [string]$RuntimePath,
    [string]$ModelPath,
    [string]$ModelAlias,
    [ValidateRange(1, 65535)]
    [int]$ServerPort = 0,
    [ValidateRange(128, 1048576)]
    [int]$ContextSize = 2048,
    [ValidateRange(0, 999)]
    [int]$GpuLayers = 0,
    [ValidateRange(1, 256)]
    [int]$Threads = 8,
    [ValidateRange(1, 32)]
    [int]$Parallel = 1,
    [int]$Seed = -1,
    [switch]$AllowFailures,
    [switch]$PlanOnly
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

# -Engine is retained for the two historical convenience names. Explicit
# -Config wins when both are supplied.
if (-not $Config) {
    switch ($Engine) {
        "mt15" { $Config = "a" }
        "mt2" { $Config = "c" }
        default { $Config = "a" }
    }
}

if (-not $ResultsDir) {
    $ResultsDir = Join-Path $repositoryRoot "eval-results"
}
if (-not $Dataset) {
    $Dataset = Join-Path $repositoryRoot "evals\subtitle-eval-v1.json"
}
if (-not $AppConfigPath) {
    $AppConfigPath = Join-Path $env:APPDATA "com.meowcal.sub\config.json"
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
if (-not $ReportPath) {
    $ReportPath = Join-Path $ResultsDir "cfg-$Config-$stamp.json"
}

if ($Config -in @("b", "c")) {
    $missing = @(
        if (-not $RuntimePath) { "RuntimePath" }
        if (-not $ModelPath) { "ModelPath" }
        if (-not $ModelAlias) { "ModelAlias" }
        if ($ServerPort -eq 0) { "ServerPort" }
    )
    if ($missing.Count -gt 0) {
        throw "Config $Config requires explicit parameters: $($missing -join ', ')"
    }
}

function Get-SubtitleEvalArguments {
    param(
        [Parameter(Mandatory)]
        [string]$ConfigPath,
        [Parameter(Mandatory)]
        [string]$ReportFile
    )

    return @(
        "run", "--locked", "--release", "--manifest-path",
        (Join-Path $repositoryRoot "src-tauri\Cargo.toml"),
        "--bin", "subtitle-eval", "--",
        "--dataset", $Dataset,
        "--live", "--config", $ConfigPath,
        "--report", $ReportFile,
        "--runs", "$Runs"
    )
}

function Get-ServerArguments {
    $arguments = @(
        "-m", $ModelPath,
        "--alias", $ModelAlias,
        "--host", "127.0.0.1",
        "--port", "$ServerPort",
        "-c", "$ContextSize",
        "-ngl", "$GpuLayers",
        "--jinja", "--no-webui",
        "--parallel", "$Parallel",
        "--threads", "$Threads"
    )
    if ($Seed -ge 0) {
        $arguments += @("--seed", "$Seed")
    }
    return $arguments
}

$configPath = Join-Path $ResultsDir "eval-config-$Config.json"
$evalArguments = Get-SubtitleEvalArguments -ConfigPath $configPath -ReportFile $ReportPath
$serverArguments = if ($Config -eq "a") { @() } else { @(Get-ServerArguments) }

if ($PlanOnly) {
    [ordered]@{
        config = $Config
        mode = if ($Config -eq "a") { "managed" } else { "isolated" }
        appConfigPath = $AppConfigPath
        dataset = $Dataset
        reportPath = $ReportPath
        evalConfigPath = $configPath
        runtimePath = if ($Config -eq "a") { $null } else { $RuntimePath }
        modelPath = if ($Config -eq "a") { $null } else { $ModelPath }
        modelAlias = if ($Config -eq "a") { $null } else { $ModelAlias }
        cargoArguments = $evalArguments
        serverArguments = $serverArguments
    } | ConvertTo-Json -Depth 5
    exit 0
}

if (-not (Test-Path -LiteralPath $Dataset -PathType Leaf)) {
    throw "Dataset not found: $Dataset"
}
if (-not (Test-Path -LiteralPath $AppConfigPath -PathType Leaf)) {
    throw "App config not found: $AppConfigPath"
}
if ($Config -in @("b", "c")) {
    if (-not (Test-Path -LiteralPath $RuntimePath -PathType Leaf)) {
        throw "Engine binary not found: $RuntimePath"
    }
    if (-not (Test-Path -LiteralPath $ModelPath -PathType Leaf)) {
        throw "Model not found: $ModelPath"
    }
    $activePorts = [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners().Port
    if ($activePorts -contains $ServerPort) {
        throw "Server port $ServerPort is already in use; refusing to adopt or stop an existing process"
    }
}

New-Item -ItemType Directory -Force -Path $ResultsDir | Out-Null
$appConfig = Get-Content -Raw -LiteralPath $AppConfigPath | ConvertFrom-Json

function Invoke-SubtitleEval {
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments,
        [Parameter(Mandatory)]
        [string]$ReportFile
    )

    & cargo @Arguments | Out-Host
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0 -and (-not $AllowFailures -or -not (Test-Path -LiteralPath $ReportFile -PathType Leaf))) {
        throw "subtitle-eval exited with $exitCode (report: $ReportFile)"
    }
    return $exitCode
}

function Write-ReportSummary {
    param(
        [Parameter(Mandatory)]
        [string]$ReportFile,
        [Parameter(Mandatory)]
        [string]$Tag
    )

    if (-not (Test-Path -LiteralPath $ReportFile -PathType Leaf)) {
        Write-Host "[$Tag] NO REPORT WRITTEN"
        return
    }
    $report = Get-Content -Raw -LiteralPath $ReportFile | ConvertFrom-Json
    Write-Host (
        "[$Tag] passed=$($report.passed) runs=$($report.runs) cases=$($report.results.Count) " +
        "p50=$($report.p50LatencyMs)ms p95=$($report.p95LatencyMs)ms warmup=$($report.warmupLatencyMs)ms " +
        "budgetOK=$($report.p50WithinBudget)/$($report.p95WithinBudget)"
    )
}

if ($Config -eq "a") {
    Copy-Item -LiteralPath $AppConfigPath -Destination $configPath -Force
    $engineId = $appConfig.translation.foundryLocal.managedRuntime.engineId
    Write-Host "[a] app-managed baseline (engineId: $engineId)"
    $evalExitCode = Invoke-SubtitleEval -Arguments $evalArguments -ReportFile $ReportPath
    Write-ReportSummary -ReportFile $ReportPath -Tag "a"
    Write-Host "Report: $ReportPath"
    exit $evalExitCode
}

# Isolated configs inherit the app wire settings, replace only the selected
# endpoint/model, and remove the app-managed runtime record.
$evalConfig = $appConfig | ConvertTo-Json -Depth 20 | ConvertFrom-Json
$evalConfig.translation.foundryLocal.model = $ModelAlias
$evalConfig.translation.foundryLocal.endpointUrl = "http://127.0.0.1:$ServerPort"
$evalConfig.translation.foundryLocal.PSObject.Properties.Remove("managedRuntime")
$evalConfig | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $configPath -Encoding utf8

$serverLog = Join-Path $ResultsDir "cfg-$Config-server-$stamp.log"
$server = $null
$serverStdoutStream = $null
$serverStderrStream = $null
$serverStdoutTask = $null
$serverStderrTask = $null
$evalExitCode = 0
try {
    Write-Host ("[{0}] starting {1} on 127.0.0.1:{2} (server seed: {3})" -f $Config, $ModelAlias, $ServerPort, $(if ($Seed -ge 0) { $Seed } else { "default" }))
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $RuntimePath
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $serverArguments) {
        $startInfo.ArgumentList.Add([string]$argument)
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        $process.Dispose()
        throw "Failed to start engine process: $RuntimePath"
    }
    $server = $process
    $serverStdoutStream = [System.IO.FileStream]::new(
        $serverLog,
        [System.IO.FileMode]::Create,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::Read
    )
    $serverStderrStream = [System.IO.FileStream]::new(
        "$serverLog.err",
        [System.IO.FileMode]::Create,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::Read
    )
    $serverStdoutTask = $server.StandardOutput.BaseStream.CopyToAsync($serverStdoutStream)
    $serverStderrTask = $server.StandardError.BaseStream.CopyToAsync($serverStderrStream)

    $healthy = $false
    for ($attempt = 0; $attempt -lt 300; $attempt++) {
        Start-Sleep -Milliseconds 500
        if ($server.HasExited) {
            break
        }
        try {
            $health = Invoke-WebRequest -Uri "http://127.0.0.1:$ServerPort/health" -TimeoutSec 2 -UseBasicParsing
            if ($health.StatusCode -eq 200) {
                $healthy = $true
                break
            }
        } catch {
            # The owned server may still be loading the model.
        }
    }
    if (-not $healthy) {
        throw "Engine server did not become healthy (log: $serverLog.err)"
    }

    Write-Host "Engine healthy; running eval"
    $evalExitCode = Invoke-SubtitleEval -Arguments $evalArguments -ReportFile $ReportPath
    Write-ReportSummary -ReportFile $ReportPath -Tag $Config
} finally {
    try {
        if ($null -ne $server) {
            $server.Refresh()
            if (-not $server.HasExited) {
                Write-Host "Stopping owned engine process (PID $($server.Id))"
                $server.Kill()
                if (-not $server.WaitForExit(10000)) {
                    throw "Owned engine process $($server.Id) did not exit within 10 seconds"
                }
            }
        }
    } finally {
        if ($null -ne $serverStdoutTask) {
            $serverStdoutTask.GetAwaiter().GetResult()
        }
        if ($null -ne $serverStderrTask) {
            $serverStderrTask.GetAwaiter().GetResult()
        }
        if ($null -ne $serverStdoutStream) {
            $serverStdoutStream.Dispose()
        }
        if ($null -ne $serverStderrStream) {
            $serverStderrStream.Dispose()
        }
        if ($null -ne $server) {
            $server.Dispose()
        }
    }
}

Write-Host "Report: $ReportPath"
exit $evalExitCode
