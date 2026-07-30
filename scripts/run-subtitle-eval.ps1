[CmdletBinding()]
param(
    [switch]$Live,
    [ValidateRange(1, 20)]
    [int]$Runs = 1,
    [string]$ConfigPath,
    [string]$ReportPath
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "src-tauri\Cargo.toml"
$datasetPath = Join-Path $repositoryRoot "evals\subtitle-eval-v1.json"
$arguments = @(
    "run",
    "--locked",
    "--manifest-path",
    $manifestPath,
    "--bin",
    "subtitle-eval",
    "--",
    "--dataset",
    $datasetPath
)

if ($Live) {
    $arguments += @("--live", "--runs", $Runs)
    if ($ConfigPath) {
        $arguments += @("--config", $ConfigPath)
    }
    if ($ReportPath) {
        $arguments += @("--report", $ReportPath)
    }
}

& cargo @arguments
exit $LASTEXITCODE
