# =============================================================================
# SAMPLE-GPU-COUNTERS.PS1 - machine-readable GPU/CPU/RAM sampler (JSONL)
# =============================================================================
# Experiment wiring for the GPU benchmark (kept in the benchmark worktree).
# Samples every ~1s:
#   - GPU Engine utilization by LUID (max engine per LUID) + global max
#   - GPU Adapter Memory: shared + dedicated usage per LUID
#   - GPU Process Memory shared usage for the target PID (absent when the
#     process has no GPU context - itself a diagnostic)
#   - Process CPU% (delta of total CPU seconds), working set, private bytes
#   - System available memory (MB)
# Stops when the stop-file <OutJsonl>.stop appears or MaxSeconds elapses.
# =============================================================================
[CmdletBinding()]
param(
    [int]$TargetPid = 0,
    [string]$OutJsonl,
    [int]$MaxSeconds = 7200,
    [int]$IntervalMs = 1000
)

$ErrorActionPreference = "Continue"
$cores = [Environment]::ProcessorCount
$stopFile = "$OutJsonl.stop"
$deadline = (Get-Date).AddSeconds($MaxSeconds)
$lastCpu = $null
$lastSampleAt = $null

function Get-Sample {
    $now = Get-Date
    $sample = [ordered]@{
        t_unix = [long]([DateTimeOffset]::Now.ToUnixTimeSeconds())
        t_local = $now.ToString("HH:mm:ss.fff")
    }

    # GPU counters (per-LUID dictionaries + global max)
    $gpuEngines = @{}
    $gpuShared = @{}
    $gpuDedicated = @{}
    $gpuProcShared = @{}
    $gpuTopInstance = @{}   # luid -> "pid:engtype:pct" of the busiest instance
    try {
        $cs = Get-Counter -Counter (
            '\GPU Engine(*)\Utilization Percentage',
            '\GPU Adapter Memory(*)\Shared Usage',
            '\GPU Adapter Memory(*)\Dedicated Usage',
            '\GPU Process Memory(*)\Shared Usage'
        ) -ErrorAction Stop
        foreach ($s in $cs.CounterSamples) {
            $path = $s.Path
            $val = [math]::Round([double]$s.CookedValue, 1)
            if ($path -match '\\GPU Engine\((.*?)\)\\Utilization Percentage') {
                $inst = $matches[1]
                if ($inst -match '^pid_(\d+)_(luid_[^_]+_[^_]+)_phys_(\d+)_eng_(\d+)_engtype_(.*)$') {
                    $luid = $matches[2]
                    $engPid = [int]$matches[1]
                    $engtype = $matches[5]
                    $gpuEngines[$luid] = [math]::Max($gpuEngines[$luid], $val)
                    if (-not $gpuTopInstance.ContainsKey($luid) -or $val -gt ($gpuTopInstance[$luid] -split ':' | Select-Object -Last 1)) {
                        $gpuTopInstance[$luid] = "${engPid}:${engtype}:${val}"
                    }
                    if ($engPid -eq $TargetPid) {
                        $key = "gpu_util_llama_pid_$luid"
                        if (-not $sample.ContainsKey($key) -or $val -gt [double]$sample[$key]) {
                            $sample[$key] = $val
                            $sample["gpu_engine_llama_$luid"] = $engtype
                        }
                    }
                }
            }
            elseif ($path -match '\\GPU Adapter Memory\((.*?)\)\\Shared Usage') {
                $gpuShared[$matches[1]] = $val
            }
            elseif ($path -match '\\GPU Adapter Memory\((.*?)\)\\Dedicated Usage') {
                $gpuDedicated[$matches[1]] = $val
            }
            elseif ($path -match '\\GPU Process Memory\((.*?)\)\\Shared Usage') {
                $inst = $matches[1]
                if ($inst -match '^pid_(\d+)_(luid_[^_]+_[^_]+)_phys_(\d+)$') {
                    if ([int]$matches[1] -eq $TargetPid) {
                        $gpuProcShared["luid_$($matches[2])"] = $val
                    }
                }
            }
        }
    }
    catch { $sample["gpu_counter_error"] = $_.Exception.Message }

    if ($gpuEngines.Count -gt 0) {
        $maxVal = ($gpuEngines.Values | Measure-Object -Maximum).Maximum
        $sample["gpu_util_max_pct"] = [math]::Round([double]$maxVal, 1)
    }
    $sample["gpu_util_by_luid"] = ($gpuEngines.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ";"
    $sample["gpu_shared_by_luid"] = ($gpuShared.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ";"
    $sample["gpu_dedicated_by_luid"] = ($gpuDedicated.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ";"
    $sample["gpu_proc_shared_llama"] = ($gpuProcShared.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ";"
    $sample["gpu_top_instance_by_luid"] = ($gpuTopInstance.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ";"

    # Process counters
    try {
        $p = Get-Process -Id $TargetPid -ErrorAction Stop
        $sample["proc_ws_mb"] = [math]::Round($p.WorkingSet64 / 1MB, 1)
        $sample["proc_priv_mb"] = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
        $sample["proc_handles"] = $p.HandleCount
        if ($null -ne $script:lastCpu -and $null -ne $script:lastSampleAt) {
            $dt = ($now - $script:lastSampleAt).TotalSeconds
            if ($dt -gt 0) {
                $cpuPct = [math]::Round((($p.CPU - $script:lastCpu) * 100.0) / ($cores * $dt), 1)
                if ($cpuPct -lt 0) { $cpuPct = 0 }
                $sample["proc_cpu_pct"] = $cpuPct
                $sample["proc_cpu_pct_single_core"] = [math]::Round($cpuPct * $cores, 1)
            }
        }
        $script:lastCpu = $p.CPU
        $script:lastSampleAt = $now
    }
    catch { $sample["proc_error"] = $_.Exception.Message }

    try {
        $os = Get-CimInstance Win32_OperatingSystem
        $sample["sys_avail_mb"] = [math]::Round($os.FreePhysicalMemory / 1KB, 0)
    }
    catch { $sample["sys_error"] = $_.Exception.Message }

    return $sample
}

while ((Get-Date) -lt $deadline) {
    if (Test-Path -LiteralPath $stopFile) { break }
    $s = Get-Sample
    $line = $s | ConvertTo-Json -Compress -Depth 4
    [System.IO.File]::AppendAllText($OutJsonl, $line + "`n", [System.Text.UTF8Encoding]::new($false))
    Start-Sleep -Milliseconds $IntervalMs
}
Write-Host "sampler-done"
