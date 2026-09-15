<#
.SYNOPSIS
Runs the historical A/B/C full-session and stability benchmark matrix.

.DESCRIPTION
Config A is the app-managed baseline. Config B is the isolated runtime
control. Config C is the isolated candidate. Runtime and model inputs for B/C
are explicit. Use PlanOnly to inspect every child command without writing
artifacts or starting an engine.

.EXAMPLE
.\scripts\run-bench-batch.ps1 -BenchDir .\eval-results\bench -FullDatasetPaths .\evals\subtitle-eval-v1.json -StabilityDatasetPaths .\evals\subtitle-eval-v1.json -AppConfigPath "$env:APPDATA\com.meowcal.sub\config.json" -ConfigBRuntimePath "D:\llama runtime\llama-server.exe" -ConfigBModelPath "D:\models\baseline.gguf" -ConfigCRuntimePath "D:\llama runtime\llama-server.exe" -ConfigCModelPath "D:\models\candidate.gguf" -ConfigCModelAlias "candidate model" -PlanOnly
#>
# A is the app-managed baseline. B is the isolated runtime control. C is the
# isolated candidate. Runtime/model inputs are explicit and forwarded to the
# single-run driver so the matrix can be reused on another machine or model.
# =============================================================================

[CmdletBinding()]
param(
    [string]$BenchDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "eval-results\bench"),
    [string]$ScriptsDir = $PSScriptRoot,
    [string]$LogDir = "",
    [string]$AppConfigPath,
    [string[]]$FullDatasetPaths = @(),
    [string[]]$StabilityDatasetPaths = @(),
    [switch]$StabilityOnly,
    [int[]]$StabilitySeeds = @(2026, 4242),
    [Parameter(Mandatory)]
    [string]$ConfigBRuntimePath,
    [Parameter(Mandatory)]
    [string]$ConfigBModelPath,
    [string]$ConfigBModelAlias = "HY-MT1.5-1.8B-Q4_K_M",
    [ValidateRange(1, 65535)]
    [int]$ConfigBServerPort = 11651,
    [ValidateRange(128, 1048576)]
    [int]$ConfigBContextSize = 2048,
    [ValidateRange(0, 999)]
    [int]$ConfigBGpuLayers = 0,
    [ValidateRange(1, 256)]
    [int]$ConfigBThreads = 8,
    [ValidateRange(1, 32)]
    [int]$ConfigBParallel = 1,
    [int]$ConfigBFullSeed = 2026,
    [Parameter(Mandatory)]
    [string]$ConfigCRuntimePath,
    [Parameter(Mandatory)]
    [string]$ConfigCModelPath,
    [string]$ConfigCModelAlias = "Hy-MT2-1.8B-Q4_K_M",
    [ValidateRange(1, 65535)]
    [int]$ConfigCServerPort = 11652,
    [ValidateRange(128, 1048576)]
    [int]$ConfigCContextSize = 2048,
    [ValidateRange(0, 999)]
    [int]$ConfigCGpuLayers = 0,
    [ValidateRange(1, 256)]
    [int]$ConfigCThreads = 8,
    [ValidateRange(1, 32)]
    [int]$ConfigCParallel = 1,
    [int]$ConfigCFullSeed = 2026,
    [switch]$PlanOnly
)

$ErrorActionPreference = "Stop"
if (-not $LogDir) {
    $LogDir = Join-Path $BenchDir "logs"
}
$reportsDir = Join-Path $BenchDir "reports"
$datasetsDir = Join-Path $BenchDir "datasets"
$driverPath = Join-Path $ScriptsDir "run-engine-eval.ps1"

if ($FullDatasetPaths.Count -eq 0 -and (Test-Path -LiteralPath $datasetsDir -PathType Container)) {
    $FullDatasetPaths = @(
        Get-ChildItem -LiteralPath $datasetsDir -File -Filter "*.json" |
            Where-Object { $_.Name -notmatch "stability" } |
            Sort-Object Name |
            Select-Object -ExpandProperty FullName
    )
}
if ($StabilityDatasetPaths.Count -eq 0 -and (Test-Path -LiteralPath $datasetsDir -PathType Container)) {
    $StabilityDatasetPaths = @(
        Get-ChildItem -LiteralPath $datasetsDir -File -Filter "*stability*.json" |
            Sort-Object Name |
            Select-Object -ExpandProperty FullName
    )
}
if (-not $StabilityOnly -and $FullDatasetPaths.Count -eq 0) {
    throw "No full-session datasets supplied or found under $datasetsDir"
}
if ($StabilityDatasetPaths.Count -eq 0) {
    throw "No stability datasets supplied or found under $datasetsDir"
}

$configArguments = @{
    a = @()
    b = @(
        "-RuntimePath", $ConfigBRuntimePath,
        "-ModelPath", $ConfigBModelPath,
        "-ModelAlias", $ConfigBModelAlias,
        "-ServerPort", "$ConfigBServerPort",
        "-ContextSize", "$ConfigBContextSize",
        "-GpuLayers", "$ConfigBGpuLayers",
        "-Threads", "$ConfigBThreads",
        "-Parallel", "$ConfigBParallel"
    )
    c = @(
        "-RuntimePath", $ConfigCRuntimePath,
        "-ModelPath", $ConfigCModelPath,
        "-ModelAlias", $ConfigCModelAlias,
        "-ServerPort", "$ConfigCServerPort",
        "-ContextSize", "$ConfigCContextSize",
        "-GpuLayers", "$ConfigCGpuLayers",
        "-Threads", "$ConfigCThreads",
        "-Parallel", "$ConfigCParallel"
    )
}

function Get-DriverArguments {
    param(
        [Parameter(Mandatory)]
        [string]$Config,
        [Parameter(Mandatory)]
        [string]$DatasetPath,
        [Parameter(Mandatory)]
        [string]$ReportFile,
        [Parameter(Mandatory)]
        [int]$Seed
    )

    $arguments = @(
        "-NoProfile", "-File", $driverPath,
        "-Config", $Config,
        "-Dataset", $DatasetPath,
        "-ReportPath", $ReportFile,
        "-ResultsDir", (Split-Path -Parent $ReportFile),
        "-AllowFailures"
    )
    if ($AppConfigPath) {
        $arguments += @("-AppConfigPath", $AppConfigPath)
    }
    $arguments += $configArguments[$Config]
    if ($Seed -ge 0) {
        $arguments += @("-Seed", "$Seed")
    }
    if ($PlanOnly) {
        $arguments += "-PlanOnly"
    }
    return $arguments
}

$plannedRuns = [System.Collections.Generic.List[object]]::new()
function Add-PlannedRun {
    param(
        [Parameter(Mandatory)]
        [string]$Config,
        [Parameter(Mandatory)]
        [string]$DatasetPath,
        [Parameter(Mandatory)]
        [int]$Seed,
        [Parameter(Mandatory)]
        [string]$Tag
    )

    $index = $plannedRuns.Count + 1
    $reportFile = Join-Path $reportsDir ("{0}-{1}-run{2:D3}.json" -f $Config, $Tag, $index)
    $plannedRuns.Add([ordered]@{
        index = $index
        config = $Config
        tag = $Tag
        seed = if ($Seed -ge 0) { $Seed } else { $null }
        datasetPath = $DatasetPath
        reportFile = $reportFile
        driverArguments = @(Get-DriverArguments -Config $Config -DatasetPath $DatasetPath -ReportFile $reportFile -Seed $Seed)
    })
}

if (-not $StabilityOnly) {
    foreach ($datasetPath in $FullDatasetPaths) {
        $tag = [IO.Path]::GetFileNameWithoutExtension($datasetPath) -replace "^s\d+_", ""
        Add-PlannedRun -Config "a" -DatasetPath $datasetPath -Seed -1 -Tag $tag
        Add-PlannedRun -Config "b" -DatasetPath $datasetPath -Seed $ConfigBFullSeed -Tag $tag
        Add-PlannedRun -Config "c" -DatasetPath $datasetPath -Seed $ConfigCFullSeed -Tag $tag
    }
}

foreach ($datasetPath in $StabilityDatasetPaths) {
    $tag = [IO.Path]::GetFileNameWithoutExtension($datasetPath) -replace "^s\d+_", "" -replace "_stability$", ""
    foreach ($seed in $StabilitySeeds) {
        Add-PlannedRun -Config "b" -DatasetPath $datasetPath -Seed $seed -Tag "$tag-stability"
        Add-PlannedRun -Config "c" -DatasetPath $datasetPath -Seed $seed -Tag "$tag-stability"
    }
    Add-PlannedRun -Config "a" -DatasetPath $datasetPath -Seed -1 -Tag "$tag-stability"
    Add-PlannedRun -Config "a" -DatasetPath $datasetPath -Seed -1 -Tag "$tag-stability"
}

if ($PlanOnly) {
    [ordered]@{
        benchDir = $BenchDir
        runCount = $plannedRuns.Count
        runs = $plannedRuns
    } | ConvertTo-Json -Depth 8
    exit 0
}

if (-not (Test-Path -LiteralPath $driverPath -PathType Leaf)) {
    throw "Eval driver not found: $driverPath"
}
foreach ($datasetPath in @($FullDatasetPaths) + @($StabilityDatasetPaths)) {
    if (-not (Test-Path -LiteralPath $datasetPath -PathType Leaf)) {
        throw "Dataset not found: $datasetPath"
    }
}

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null
$manifest = @()
$manifestPath = Join-Path $BenchDir "bench-manifest.json"

foreach ($run in $plannedRuns) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $reportFile = Join-Path $reportsDir ("{0}-{1}-{2:D3}-{3}.json" -f $run.config, $run.tag, $run.index, $stamp)
    $logFile = Join-Path $LogDir ("{0}-{1}-{2:D3}-{3}.log" -f $run.config, $run.tag, $run.index, $stamp)
    $seed = if ($null -eq $run.seed) { -1 } else { $run.seed }
    $driverArguments = Get-DriverArguments -Config $run.config -DatasetPath $run.datasetPath -ReportFile $reportFile -Seed $seed

    Write-Host ("[{0}] run {1}: config={2} dataset={3} seed={4}" -f $stamp, $run.index, $run.config, (Split-Path -Leaf $run.datasetPath), $(if ($null -eq $run.seed) { "default" } else { $run.seed }))
    $started = Get-Date
    $output = & pwsh @driverArguments 2>&1
    $exitCode = $LASTEXITCODE
    $elapsed = ((Get-Date) - $started).TotalSeconds
    $output | Out-File -LiteralPath $logFile -Encoding utf8

    $summary = $null
    if (Test-Path -LiteralPath $reportFile -PathType Leaf) {
        $report = Get-Content -Raw -LiteralPath $reportFile | ConvertFrom-Json
        $summary = [ordered]@{
            passed = $report.passed
            runs = $report.runs
            cases = $report.results.Count
            p50LatencyMs = $report.p50LatencyMs
            p95LatencyMs = $report.p95LatencyMs
            warmupLatencyMs = $report.warmupLatencyMs
            p50WithinBudget = $report.p50WithinBudget
            p95WithinBudget = $report.p95WithinBudget
            modelId = $report.modelId
            engineVersion = $report.engineVersion
        }
    }

    $manifest += [ordered]@{
        index = $run.index
        config = $run.config
        tag = $run.tag
        seed = $run.seed
        dataset = Split-Path -Leaf $run.datasetPath
        reportFile = if (Test-Path -LiteralPath $reportFile -PathType Leaf) { Split-Path -Leaf $reportFile } else { $null }
        logFile = Split-Path -Leaf $logFile
        exitCode = $exitCode
        elapsedSec = [math]::Round($elapsed, 1)
        summary = $summary
    }
    $manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8
    Write-Host ("      -> exit={0} elapsed={1}s report={2}" -f $exitCode, [math]::Round($elapsed, 1), $(if (Test-Path -LiteralPath $reportFile -PathType Leaf) { Split-Path -Leaf $reportFile } else { "MISSING" }))
}

Write-Host ""
Write-Host "Battery complete: $($plannedRuns.Count) runs"
Write-Host "Manifest: $manifestPath"
