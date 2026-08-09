# =============================================================================
# ATTRIBUTE-FAILURES.PS1 - meaningful vs OCR-noise failure attribution
# =============================================================================
# Joins review/blind-results.json (decoded scores) with the reviewer's source
# classification (review/blind-scores/src-class-*.json) and writes
# review/failure-attribution.md.
# =============================================================================

[CmdletBinding()]
param([string]$BenchDir = "")

$ErrorActionPreference = "Stop"
if (-not $BenchDir) { $BenchDir = Join-Path (Split-Path -Parent $PSScriptRoot) "eval-results\bench" }

$reviewDir = Join-Path $BenchDir "review"
$results = Get-Content -Raw -LiteralPath (Join-Path $reviewDir "blind-results.json") | ConvertFrom-Json
$rows = @($results.rows)

$kinds = @{}
foreach ($f in @(Get-ChildItem -LiteralPath (Join-Path $reviewDir "blind-scores") -Filter "src-class-*.json")) {
    foreach ($e in @(Get-Content -Raw -LiteralPath $f.FullName | ConvertFrom-Json)) {
        $kinds[$e.caseId] = $e.kind
    }
}
if ($kinds.Count -ne $rows.Count) { Write-Warning "kind count $($kinds.Count) != row count $($rows.Count)" }

$out = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
    perCase = @()
    byKind = [ordered]@{}
}

$cfgNames = @('a', 'b', 'c')
foreach ($kind in @('meaningful', 'noise')) {
    $subset = @($rows | Where-Object { $kinds[$_.caseId] -eq $kind })
    $counts = [ordered]@{}
    foreach ($cfg in $cfgNames) {
        $serious = @($subset | Where-Object { $_.$cfg -le 2 })
        $counts[$cfg] = $serious.Count
    }
    $out.byKind[$kind] = [ordered]@{
        cases        = $subset.Count
        seriousA      = $counts.a
        seriousB      = $counts.b
        seriousC      = $counts.c
        rateA        = if ($subset.Count) { [math]::Round(100.0 * $counts.a / $subset.Count, 1) } else { 0 }
        rateB        = if ($subset.Count) { [math]::Round(100.0 * $counts.b / $subset.Count, 1) } else { 0 }
        rateC        = if ($subset.Count) { [math]::Round(100.0 * $counts.c / $subset.Count, 1) } else { 0 }
        seriousCases = @($subset | Where-Object { $_.a -le 2 -or $_.b -le 2 -or $_.c -le 2 } | ForEach-Object {
            [ordered]@{
                caseId = $_.caseId
                kind   = $kind
                a      = $_.a
                b      = $_.b
                c      = $_.c
                note   = $_.note
            }
        })
    }
}

$out | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $reviewDir "attribution.json") -Encoding utf8

$md = @()
$md += "# Failure attribution (blind review)"
$md += ""
foreach ($kind in @('meaningful', 'noise')) {
    $v = $out.byKind[$kind]
    $md += "## $kind lines ($($v.cases) sampled cases)"
    $md += ""
    $md += "| config | serious (<=2) | rate |"
    $md += "|---|---|---|"
    $md += "| a | $($v.seriousA) | $($v.rateA)% |"
    $md += "| b | $($v.seriousB) | $($v.rateB)% |"
    $md += "| c | $($v.seriousC) | $($v.rateC)% |"
    $md += ""
    $md += "| case | a | b | c | note |"
    $md += "|---|---|---|---|---|"
    foreach ($c in $v.seriousCases) {
        $md += "| $($c.caseId) | $($c.a) | $($c.b) | $($c.c) | $($c.note) |"
    }
    $md += ""
}
$md | Set-Content -LiteralPath (Join-Path $reviewDir "attribution.md") -Encoding utf8
Write-Host "wrote attribution.json + attribution.md"