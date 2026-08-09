# =============================================================================
# ANALYZE-BENCH.PS1 - aggregate benchmark reports into metrics + markdown
# =============================================================================
# Consumes eval-results/bench/bench-manifest.json (optional; produced by a
# benchmark orchestrator) together with per-run report JSONs and produces:
#   aggregate.json  - per config/dataset metrics incl. failure-mode groups
#   aggregate.md    - human-readable digest
#
# If the manifest is missing or empty, the run list is reconstructed from the
# reports/ directory file names, which encode <config>-<tag>-<stamp>.json, so
# this script works directly with reports emitted by `subtitle-eval --report`.
# =============================================================================

[CmdletBinding()]
param(
    [string]$BenchDir = "",
    [string]$SelectionReport = ""
)

$ErrorActionPreference = "Stop"
if (-not $BenchDir) { $BenchDir = Join-Path (Split-Path -Parent $PSScriptRoot) "eval-results\bench" }
if (-not $SelectionReport) { $SelectionReport = Join-Path $BenchDir "selection-report.json" }

function Get-LatencyStats {
    param($Latencies)
    $sorted = @($Latencies | Sort-Object)
    if ($sorted.Count -eq 0) { return $null }
    $count = $sorted.Count
    $p = { param($idx) $sorted[[Math]::Min($idx - 1, $count - 1)] }
    [ordered]@{
        count   = $count
        p50     = & $p ([math]::Ceiling($count * 0.50))
        p90     = & $p ([math]::Ceiling($count * 0.90))
        p95     = & $p ([math]::Ceiling($count * 0.95))
        p99     = & $p ([math]::Ceiling($count * 0.99))
        max     = ($sorted | Select-Object -Last 1)
    }
}

function Get-FailedCauses {
    param($Failed)
    @($Failed | ForEach-Object { $_.reason } | Group-Object | ForEach-Object {
        [pscustomobject]@{ reason = $_.Name; count = $_.Count }
    } | Sort-Object count -Descending)
}

# --------------------------------------------------------------- load runs ---
$manifestPath = Join-Path $BenchDir "bench-manifest.json"
$runs = @()
if (Test-Path -LiteralPath $manifestPath) {
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    foreach ($manifestEntry in $manifest) {
        if (-not $manifestEntry.reportFile) { continue }
        $reportPath = Join-Path $BenchDir "reports\$($manifestEntry.reportFile)"
        if (-not (Test-Path -LiteralPath $reportPath)) { continue }
        $runs += [ordered]@{
            config    = $manifestEntry.config
            seed      = $manifestEntry.seed
            tag       = $manifestEntry.tag
            dataset   = $manifestEntry.dataset
            reportFile = $manifestEntry.reportFile
            stability = ($manifestEntry.tag -match "stability")
        }
    }
}

if (-not $runs.Count) {
    Write-Host "bench-manifest.json missing/empty - reconstructing run list from reports dir"
    $reportsDir = Join-Path $BenchDir "reports"
    $reportRe = '^(?<config>[abc])-(?<tag>.+?)-\d{8}-\d{6}\.json$'
    foreach ($reportFile in @(Get-ChildItem -LiteralPath $reportsDir -File -Filter "*.json" | Where-Object { $_.Name -match '^(a|b|c)-' })) {
        if ($reportFile.Name -notmatch $reportRe) { continue }
        $runs += [ordered]@{
            config     = $Matches.config
            seed       = $null
            tag        = $Matches.tag
            dataset    = $null
            reportFile = $reportFile.Name
            stability  = ($Matches.tag -match "stability")
        }
    }
}

if (-not $runs.Count) { Write-Error "No run reports found under $BenchDir\reports - produce eval reports first (e.g. via subtitle-eval --report)" }

# ------------------------------------------------------ per-run metrics ----
$runAnalysis = @()
foreach ($runInfo in $runs) {
    $reportPath = Join-Path $BenchDir "reports\$($runInfo.reportFile)"
    $report = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    $latencies = @($report.results | ForEach-Object { [double]$_.latencyMs })
    $failed = @($report.results | Where-Object { -not $_.passed })
    $latencyStats = Get-LatencyStats -Latencies $latencies

    $runMetrics = [ordered]@{
        config             = $runInfo.config
        seed               = $runInfo.seed
        tag                = $runInfo.tag
        reportFile         = $runInfo.reportFile
        stability          = $runInfo.stability
        passed             = $report.passed
        runs               = $report.runs
        translatedAttempts = $report.translatedAttempts
        filteredCases      = $report.filteredCases
        warmupLatencyMs    = $report.warmupLatencyMs
        warmupPassed       = $report.warmupPassed
        latencies          = $latencyStats
        failureCount       = $failed.Count
        failureRate        = if ($report.translatedAttempts) { [math]::Round(100.0 * $failed.Count / $report.translatedAttempts, 2) } else { 0 }
        failuresByReason   = Get-FailedCauses -Failed $failed
        sampleOutputs      = @($report.results | ForEach-Object { $_.output })
    }
    $runAnalysis += $runMetrics
}

# ----------------------------------------------------------------- stability --
# For each config+tag pair with 2+ runs, compare outputs across runs to
# measure output stability (identical-output percentage).
$stability = [ordered]@{}
foreach ($group in @($runAnalysis | Group-Object { "$($_.config)|$($_.tag)" })) {
    $runsForPair = @($group.Group)
    if ($runsForPair.Count -lt 2) { continue }
    $maps = @()
    foreach ($r in $runsForPair) {
        $rl = Get-Content -Raw -LiteralPath (Join-Path $BenchDir "reports\$($r.reportFile)") | ConvertFrom-Json
        $h = @{}
        foreach ($case in $rl.results) { $h[$case.caseId] = $case.output }
        $maps += $h
    }
    $allCases = @(($maps[0].Keys + $maps[1].Keys) | Sort-Object -Unique)
    $changed = 0
    foreach ($c in $allCases) {
        if ($maps[0][$c] -ne $maps[1][$c]) { $changed++ }
    }
    $stability[$group.Name] = [ordered]@{
        runs              = $runsForPair.Count
        cases             = $allCases.Count
        changedAcrossRuns = $changed
        stabilityPercent  = if ($allCases.Count) { [math]::Round(100.0 * ($allCases.Count - $changed) / $allCases.Count, 2) } else { 100 }
    }
}

$aggregate = [ordered]@{}
$aggregate.generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
$aggregate.runs = $runAnalysis
$aggregate.stability = $stability

$outPath = Join-Path $BenchDir "aggregate.json"
$aggregate | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $outPath -Encoding utf8
Write-Host "aggregate: $outPath"

# ------------------------------------------------------------------ markdown --
$md = @()
$md += "# Real-session benchmark aggregate"
$md += ""
$md += "Generated: $($aggregate.generatedAtUtc)"
$md += ""
foreach ($configGroup in @($runAnalysis | Where-Object { -not $_.stability } | Group-Object -Property { $_.config })) {
    $md += "## Config $($configGroup.Name)"
    $md += ""
    $md += "| tag | attempts | p50 | p95 | p99 | max | failures | reasons | warmup |"
    $md += "|---|---|---|---|---|---|---|---|---|"
    foreach ($run in $configGroup.Group) {
        $reasons = ($run.failuresByReason | ForEach-Object { "$($_.reason)x$($_.count)" }) -join ", "
        if (-not $reasons) { $reasons = "-" }
        $md += "| $($run.tag) | $($run.translatedAttempts) | $($run.latencies.p50)ms | $($run.latencies.p95)ms | $($run.latencies.p99)ms | $($run.latencies.max)ms | $($run.failureCount) | $reasons | $($run.warmupLatencyMs)ms |"
    }
    $md += ""
}
$md += "## Stability (identical output across the runs of each config+tag)"
$md += ""
$md += "| config+tag | runs | cases | changed | stable % |"
$md += "|---|---|---|---|---|"
foreach ($key in $stability.Keys) {
    $v = $stability[$key]
    $md += "| $key | $($v.runs) | $($v.cases) | $($v.changedAcrossRuns) | $($v.stabilityPercent)% |"
}
$md += ""
$md | Set-Content -LiteralPath (Join-Path $BenchDir "aggregate.md") -Encoding utf8
Write-Host "aggregate.md written"