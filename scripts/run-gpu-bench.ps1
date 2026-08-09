# =============================================================================
# RUN-GPU-BENCH.PS1 - controlled A/B (optionally C) GPU benchmark driver
# =============================================================================
# Experiment wiring for the Adreno GPU benchmark (kept in the benchmark
# worktree, not committed to main). Runs the production eval harness
# (subtitle-eval) against the SAME llama.cpp b10155 runtime and SAME model,
# with ONLY the GPU offload configuration differing between arms:
#
#   Arm A: -ngl 0   (production CPU baseline)
#   Arm B: -ngl 99  (maximum practical GPU offload)
#   Arm C: -ngl N   (partial offload; only if B needs investigation)
#
# Everything else matches production: launch args from the manifest
# (-c 2048 --jinja --no-webui --parallel 1 --threads 8 on this 12-core host),
# the app's config wire contract, the eval warm-up policy, and the dataset.
#
# Usage:
#   .\scripts\run-gpu-bench.ps1 -Arm a -Runs 3 -Dataset <path> [-SampleCounters]
#   .\scripts\run-gpu-bench.ps1 -Arm b -Ngl 99 -Runs 3 -Dataset <path> [-SampleCounters]
#   .\scripts\run-gpu-bench.ps1 -Arm c -Ngl 24 -Runs 3 -Dataset <path> [-SampleCounters]
#
# Artifacts (gitignored): eval-results\gpu-bench\<arm>-ngl<N>-<stamp>\
#   server.log / server.err.log   engine stdout/stderr (incl. per-request timings)
#   gpu-counters.jsonl            sampler output (if -SampleCounters)
#   eval-report.json              subtitle-eval live report
#   run-summary.json              machine-readable arm metadata + health timing
# =============================================================================
[CmdletBinding()]
param(
    [ValidateSet("a", "b", "c")]
    [string]$Arm = "a",
    [int]$Ngl = -1,
    [int]$Threads = 8,
    [ValidateRange(1, 20)]
    [int]$Runs = 1,
    [string]$Dataset,
    [int]$ServerPort = 0,
    [int]$Seed = -1,
    [switch]$SampleCounters,
    [string]$OutDir,
    [string]$RuntimeExe = "C:\FormerD\foundry-cache\meowcal-sub\runtime\llama-b10155-opencl-adreno-arm64\llama-server.exe",
    [string]$ModelPath = "C:\FormerD\foundry-cache\meowcal-sub\models\hy-mt1.5-1.8b-q4\HY-MT1.5-1.8B-Q4_K_M.gguf",
    [string]$AppConfigPath,
    [int]$HealthTimeoutSeconds = 300,
    [string[]]$ExtraArgs = @()
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

if ($Ngl -lt 0) {
    switch ($Arm) {
        "a" { $Ngl = 0 }
        "b" { $Ngl = 99 }
        "c" { $Ngl = 24 }
    }
}
if ($ServerPort -eq 0) {
    switch ($Arm) {
        "a" { $ServerPort = 11440 }
        "b" { $ServerPort = 11441 }
        "c" { $ServerPort = 11442 }
    }
}
if (-not $Dataset) {
    $Dataset = Join-Path $repositoryRoot "eval-results\bench\datasets\s3_2026-08-05_22-00-04.json"
}
if (-not (Test-Path -LiteralPath $Dataset)) { throw "Dataset not found: $Dataset" }
if (-not (Test-Path -LiteralPath $RuntimeExe)) { throw "Engine binary missing: $RuntimeExe" }
if (-not (Test-Path -LiteralPath $ModelPath)) { throw "Model missing: $ModelPath" }
if (-not $AppConfigPath) { $AppConfigPath = Join-Path $env:APPDATA "com.meowcal.sub\config.json" }
if (-not (Test-Path -LiteralPath $AppConfigPath)) { throw "App config not found: $AppConfigPath" }

# A port already in use means a stray engine or the app is running - refuse to
# benchmark against something we did not start.
$inUse = Get-NetTCPConnection -LocalPort $ServerPort -State Listen -ErrorAction SilentlyContinue
if ($inUse) { throw "Port $ServerPort already in use (PID $($inUse.OwningProcess)) - refusing to race a stray engine" }

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
if (-not $OutDir) {
    $OutDir = Join-Path $repositoryRoot "eval-results\gpu-bench\$Arm-ngl$Ngl-$stamp"
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$sha = { param($p) (Get-FileHash -Algorithm SHA256 -LiteralPath $p).Hash }
$summary = [ordered]@{
    arm = $Arm
    ngl = $Ngl
    threads = $Threads
    seed = if ($Seed -ge 0) { $Seed } else { "default" }
    port = $ServerPort
    runs = $Runs
    dataset = $Dataset
    runtime_exe = $RuntimeExe
    runtime_sha256 = & $sha $RuntimeExe
    model_path = $ModelPath
    model_sha256 = & $sha $ModelPath
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    host = $env:COMPUTERNAME
    arch = $env:PROCESSOR_ARCHITECTURE
    cores = [Environment]::ProcessorCount
    os = [System.Environment]::OSVersion.VersionString
}

# ---------------------------------------------------------------------------
# Build the eval client config: same wire contract as the app config, endpoint
# swapped to this server, managed path record removed (eval starts nothing).
# ---------------------------------------------------------------------------
$appConfig = Get-Content -Raw -LiteralPath $AppConfigPath | ConvertFrom-Json
$evalConfig = $appConfig | ConvertTo-Json -Depth 20 | ConvertFrom-Json
$alias = "HY-MT1.5-1.8B-Q4_K_M"
$evalConfig.translation.foundryLocal.model = $alias
$evalConfig.translation.foundryLocal.endpointUrl = "http://127.0.0.1:$ServerPort"
$evalConfig.translation.foundryLocal.PSObject.Properties.Remove("managedRuntime")
$evalConfigPath = Join-Path $OutDir "eval-config.json"
$evalConfig | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $evalConfigPath -Encoding utf8

# ---------------------------------------------------------------------------
# Start the engine with production launch args + the arm's offload config
# ---------------------------------------------------------------------------
$serverArgs = @(
    "-m", $ModelPath,
    "--alias", $alias,
    "--host", "127.0.0.1",
    "--port", "$ServerPort",
    "-c", "2048",
    "-ngl", "$Ngl",
    "--jinja", "--no-webui", "--parallel", "1",
    "--threads", "$Threads"
)
if ($Seed -ge 0) { $serverArgs += @("--seed", "$Seed") }
if ($ExtraArgs.Count -gt 0) { $serverArgs += $ExtraArgs }
$summary["launch_args"] = $serverArgs

$serverLog = Join-Path $OutDir "server.log"
$serverErr = Join-Path $OutDir "server.err.log"
Write-Host "[$Arm] starting llama-server -ngl $Ngl on 127.0.0.1:$ServerPort (threads $Threads)"
$spawnStarted = Get-Date
$server = Start-Process -FilePath $RuntimeExe -ArgumentList $serverArgs `
    -RedirectStandardOutput $serverLog -RedirectStandardError $serverErr `
    -WindowStyle Hidden -PassThru
$summary["server_pid"] = $server.Id
Write-Host "[$Arm] server PID $($server.Id)"

# Sampler (child process; stop-file driven, killed on teardown)
$samplerProc = $null
if ($SampleCounters) {
    $countersFile = Join-Path $OutDir "gpu-counters.jsonl"
    $summary["counters_file"] = $countersFile
    $samplerScript = Join-Path $PSScriptRoot "sample-gpu-counters.ps1"
    $samplerArgs = @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $samplerScript,
        "-TargetPid", "$($server.Id)", "-OutJsonl", $countersFile
    )
    $samplerProc = Start-Process -FilePath "powershell.exe" -ArgumentList $samplerArgs `
        -WindowStyle Hidden -PassThru
    $summary["sampler_pid"] = $samplerProc.Id
    Write-Host "[$Arm] sampler PID $($samplerProc.Id)"
}

# ---------------------------------------------------------------------------
# Wait for health; record startup timing
# ---------------------------------------------------------------------------
$healthy = $false
$healthMs = -1
try {
    $waitStarted = Get-Date
    for ($attempt = 0; $attempt -lt ($HealthTimeoutSeconds * 2); $attempt++) {
        Start-Sleep -Milliseconds 500
        if ($server.HasExited) {
            throw "Engine exited during startup (code $($server.ExitCode)); log: $serverErr"
        }
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:$ServerPort/health" -TimeoutSec 2 -UseBasicParsing
            if ($r.StatusCode -eq 200) {
                $healthy = $true
                break
            }
        } catch { }
    }
    $healthMs = [math]::Round(((Get-Date) - $waitStarted).TotalMilliseconds, 0)
    $summary["health_ready_ms"] = $healthMs
    $summary["health_ready_after_spawn_ms"] = [math]::Round(((Get-Date) - $spawnStarted).TotalMilliseconds, 0)
    if (-not $healthy) { throw "Engine did not become healthy within ${HealthTimeoutSeconds}s (log: $serverErr)" }
    Write-Host "[$Arm] healthy in ${healthMs}ms"

    # -----------------------------------------------------------------------
    # Run the eval harness (single fixed release binary for all arms)
    # -----------------------------------------------------------------------
    $evalExe = Join-Path $repositoryRoot "src-tauri\target\release\subtitle-eval.exe"
    if (-not (Test-Path -LiteralPath $evalExe)) {
        $evalExe = Join-Path $repositoryRoot "src-tauri\target\debug\subtitle-eval.exe"
    }
    if (-not (Test-Path -LiteralPath $evalExe)) { throw "subtitle-eval binary not built: $evalExe" }
    $summary["eval_exe"] = $evalExe

    $reportPath = Join-Path $OutDir "eval-report.json"
    $evalStarted = Get-Date
    & $evalExe --dataset $Dataset --live --config $evalConfigPath --report $reportPath --runs $Runs
    $evalExit = $LASTEXITCODE
    $summary["eval_exit_code"] = $evalExit
    $summary["eval_elapsed_s"] = [math]::Round(((Get-Date) - $evalStarted).TotalSeconds, 1)
    Write-Host "[$Arm] eval exit $evalExit in $($summary['eval_elapsed_s'])s"
    if (-not (Test-Path -LiteralPath $reportPath)) {
        throw "No eval report written (exit $evalExit)"
    }
    $summary["report_path"] = $reportPath
} finally {
    if ($null -ne $samplerProc) {
        $stopFile = "$($summary['counters_file']).stop"
        New-Item -ItemType File -Force -Path $stopFile | Out-Null
        $samplerProc.WaitForExit(15000) | Out-Null
        if (-not $samplerProc.HasExited) {
            Stop-Process -Id $samplerProc.Id -Force -ErrorAction SilentlyContinue
        }
    }
    if (-not $server.HasExited) {
        Write-Host "[$Arm] stopping engine PID $($server.Id)"
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
        $server.WaitForExit(10000) | Out-Null
    }
}

$summary["finished_utc"] = (Get-Date).ToUniversalTime().ToString("o")
$summaryPath = Join-Path $OutDir "run-summary.json"
$summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $summaryPath -Encoding utf8
Write-Host "[$Arm] artifacts: $OutDir"
Write-Host "[$Arm] summary: $summaryPath"
