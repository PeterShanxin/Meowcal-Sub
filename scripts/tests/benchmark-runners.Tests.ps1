[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$scriptsDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$engineRunner = Join-Path $scriptsDir "run-engine-eval.ps1"
$batchRunner = Join-Path $scriptsDir "run-bench-batch.ps1"
$argumentProbe = Join-Path $PSScriptRoot "benchmark-argument-probe.ps1"
$failures = [System.Collections.Generic.List[string]]::new()

function Assert-True {
    param(
        [Parameter(Mandatory)]
        [bool]$Condition,
        [Parameter(Mandatory)]
        [string]$Message
    )

    if (-not $Condition) {
        $failures.Add($Message)
    }
}

function Get-ArgumentValue {
    param(
        [Parameter(Mandatory)]
        [object[]]$Arguments,
        [Parameter(Mandatory)]
        [string]$Name
    )

    $index = [Array]::IndexOf($Arguments, $Name)
    if ($index -lt 0 -or $index + 1 -ge $Arguments.Count) {
        return $null
    }
    return $Arguments[$index + 1]
}

foreach ($path in @($engineRunner, $batchRunner)) {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
    Assert-True ($errors.Count -eq 0) "$path has $($errors.Count) parser error(s)"
}

$missingOutput = & pwsh -NoProfile -File $engineRunner -Config b -PlanOnly 2>&1
Assert-True ($LASTEXITCODE -ne 0) "Config b accepted missing isolated-engine parameters"
Assert-True (($missingOutput -join [Environment]::NewLine) -match "RuntimePath.*ModelPath.*ModelAlias.*ServerPort") "Missing-parameter error did not list all required inputs"

$engineArgs = @(
    "-NoProfile", "-File", $engineRunner,
    "-Config", "b",
    "-RuntimePath", (Join-Path $scriptsDir "runtime with spaces\llama-server.exe"),
    "-ModelPath", (Join-Path $scriptsDir "models with spaces\candidate model.gguf"),
    "-ModelAlias", "future candidate",
    "-ServerPort", "45678",
    "-ContextSize", "4096",
    "-GpuLayers", "12",
    "-Threads", "6",
    "-Parallel", "2",
    "-Dataset", (Join-Path $scriptsDir "fixtures\synthetic.json"),
    "-AppConfigPath", (Join-Path $scriptsDir "config\app.json"),
    "-ResultsDir", (Join-Path $scriptsDir "results"),
    "-ReportPath", (Join-Path $scriptsDir "results\report.json"),
    "-Seed", "99",
    "-PlanOnly"
)
$engineJson = & pwsh @engineArgs
Assert-True ($LASTEXITCODE -eq 0) "Config b plan failed"
$enginePlan = $engineJson | ConvertFrom-Json
Assert-True ($enginePlan.mode -eq "isolated") "Config b plan was not isolated"
Assert-True ($enginePlan.runtimePath -eq (Join-Path $scriptsDir "runtime with spaces\llama-server.exe")) "RuntimePath was not preserved"
Assert-True ($enginePlan.modelAlias -eq "future candidate") "ModelAlias was not preserved"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "--port") -eq "45678") "ServerPort was not forwarded"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "-c") -eq "4096") "ContextSize was not forwarded"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "-ngl") -eq "12") "GpuLayers was not forwarded"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "--threads") -eq "6") "Threads was not forwarded"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "--parallel") -eq "2") "Parallel was not forwarded"
Assert-True ((Get-ArgumentValue $enginePlan.serverArguments "--seed") -eq "99") "Seed was not forwarded"
Assert-True (($engineJson -join [Environment]::NewLine) -notmatch "C:\\FormerD") "Engine plan contains a retired machine-specific default"

$managedJson = & pwsh -NoProfile -File $engineRunner -Config a -Dataset (Join-Path $scriptsDir "fixtures\synthetic.json") -AppConfigPath (Join-Path $scriptsDir "config\app.json") -ResultsDir (Join-Path $scriptsDir "results") -PlanOnly
Assert-True ($LASTEXITCODE -eq 0) "Config a plan failed"
$managedPlan = $managedJson | ConvertFrom-Json
Assert-True ($managedPlan.mode -eq "managed") "Config a plan was not managed"
Assert-True ($managedPlan.serverArguments.Count -eq 0) "Config a unexpectedly planned an isolated server"

$planRoot = Join-Path $env:TEMP "meowcal-benchmark-plan-only-does-not-create"
if (Test-Path -LiteralPath $planRoot) {
    throw "Test precondition failed: $planRoot already exists"
}
$batchArgs = @(
    "-NoProfile", "-File", $batchRunner,
    "-BenchDir", $planRoot,
    "-FullDatasetPaths", (Join-Path $scriptsDir "fixtures\full.json"),
    "-StabilityDatasetPaths", (Join-Path $scriptsDir "fixtures\stability.json"),
    "-ConfigBRuntimePath", (Join-Path $scriptsDir "runtime with spaces\b.exe"),
    "-ConfigBModelPath", (Join-Path $scriptsDir "models with spaces\b model.gguf"),
    "-ConfigBModelAlias", "baseline-control",
    "-ConfigBServerPort", "45151",
    "-ConfigCRuntimePath", (Join-Path $scriptsDir "runtime with spaces\c.exe"),
    "-ConfigCModelPath", (Join-Path $scriptsDir "models with spaces\c model.gguf"),
    "-ConfigCModelAlias", "future-candidate",
    "-ConfigCServerPort", "45152",
    "-PlanOnly"
)
$batchJson = & pwsh @batchArgs
Assert-True ($LASTEXITCODE -eq 0) "Batch plan failed"
$batchPlan = $batchJson | ConvertFrom-Json
Assert-True ($batchPlan.runCount -eq 9) "Historical one-full/one-stability matrix should contain 9 runs"
Assert-True ((@($batchPlan.runs | Where-Object config -eq "a")).Count -eq 3) "Batch plan did not retain three managed A runs"
Assert-True ((@($batchPlan.runs | Where-Object config -eq "b")).Count -eq 3) "Batch plan did not retain three B runs"
Assert-True ((@($batchPlan.runs | Where-Object config -eq "c")).Count -eq 3) "Batch plan did not retain three C runs"
foreach ($run in @($batchPlan.runs | Where-Object config -in @("b", "c"))) {
    Assert-True ($run.driverArguments -contains "-RuntimePath") "Isolated run $($run.index) omitted RuntimePath"
    Assert-True ($run.driverArguments -contains "-ModelPath") "Isolated run $($run.index) omitted ModelPath"
    Assert-True ($run.driverArguments -contains "-ModelAlias") "Isolated run $($run.index) omitted ModelAlias"
    Assert-True ($run.driverArguments -contains "-ServerPort") "Isolated run $($run.index) omitted ServerPort"
    Assert-True ($run.driverArguments -contains "-PlanOnly") "Plan-only child run $($run.index) could execute an engine"
}
Assert-True (-not (Test-Path -LiteralPath $planRoot)) "PlanOnly created an output directory"
Assert-True (($batchJson -join [Environment]::NewLine) -notmatch "C:\\FormerD") "Batch plan contains a retired machine-specific default"

$probeInfo = [System.Diagnostics.ProcessStartInfo]::new()
$probeInfo.FileName = (Get-Command pwsh).Source
$probeInfo.UseShellExecute = $false
$probeInfo.RedirectStandardOutput = $true
$probeInfo.RedirectStandardError = $true
foreach ($argument in @("-NoProfile", "-File", $argumentProbe, "model path with spaces.gguf", "alias with spaces")) {
    $probeInfo.ArgumentList.Add($argument)
}
$probe = [System.Diagnostics.Process]::Start($probeInfo)
$probeOutput = $probe.StandardOutput.ReadToEnd()
$probeError = $probe.StandardError.ReadToEnd()
$probe.WaitForExit()
Assert-True ($probe.ExitCode -eq 0) "Argument probe failed: $probeError"
$probeValues = @($probeOutput | ConvertFrom-Json)
Assert-True ($probeValues[0] -eq "model path with spaces.gguf") "ProcessStartInfo split a model path containing spaces"
Assert-True ($probeValues[1] -eq "alias with spaces") "ProcessStartInfo split a model alias containing spaces"

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host "benchmark runner checks passed"
