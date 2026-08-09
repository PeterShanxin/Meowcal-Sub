# =============================================================================
# AGGREGATE-BLIND.PS1 - decode blind scores against the key, aggregate, report
# =============================================================================
# Consumes review/blind-scores/scores-*.json (reviewer 1-5 X/Y/Z scores) and
# review/blind-key.json (label->config map). Writes:
#   review/blind-results.json - per-case decoded scores per config
#   review/blind-results.md   - per-config/session metrics + win/tie/loss
# =============================================================================

[CmdletBinding()]
param([string]$BenchDir = "")

$ErrorActionPreference = "Stop"
if (-not $BenchDir) { $BenchDir = Join-Path (Split-Path -Parent $PSScriptRoot) "eval-results\bench" }

$reviewDir = Join-Path $BenchDir "review"
$scoresDir = Join-Path $reviewDir "blind-scores"
$keyPath = Join-Path $reviewDir "blind-key.json"
if (-not (Test-Path -LiteralPath $keyPath)) { Write-Error "blind-key.json not found" }

$key = Get-Content -Raw -LiteralPath $keyPath | ConvertFrom-Json

$rows = @()
foreach ($sf in @(Get-ChildItem -LiteralPath $scoresDir -Filter "scores-*.json" | Sort-Object Name)) {
    foreach ($s in @(Get-Content -Raw -LiteralPath $sf.FullName | ConvertFrom-Json)) {
        $k = $key.($s.caseId)
        if (-not $k) { Write-Warning "no key for $($s.caseId)"; continue }
        $byCfg = [ordered]@{ a = $null; b = $null; c = $null }
        foreach ($label in @('X', 'Y', 'Z')) {
            $cfg = $k.labelOf.$label
            $byCfg[$cfg] = [int]$s.$label
        }
        $rows += [pscustomobject]@{
            caseId  = $s.caseId
            session = $k.session
            note    = if ($s.note) { $s.note } else { "" }
            a       = $byCfg.a
            b       = $byCfg.b
            c       = $byCfg.c
        }
    }
}
Write-Host "decoded $($rows.Count) rows"

function Get-Summary {
    param($Rows)
    [ordered]@{
        config      = "all"
        cases       = $Rows.Count
        avgA        = [math]::Round(($Rows | Measure-Object -Property a -Average).Average, 2)
        avgB        = [math]::Round(($Rows | Measure-Object -Property b -Average).Average, 2)
        avgC        = [math]::Round(($Rows | Measure-Object -Property c -Average).Average, 2)
        seriousA    = @($Rows | Where-Object { $_.a -le 2 }).Count
        seriousB    = @($Rows | Where-Object { $_.b -le 2 }).Count
        seriousC    = @($Rows | Where-Object { $_.c -le 2 }).Count
    }
}

function Get-Pair {
    param($Rows, [string]$A, [string]$B)
    $wA = 0; $wB = 0; $ties = 0
    foreach ($r in $Rows) {
        if ($r.$A -gt $r.$B) { $wA++ }
        elseif ($r.$B -gt $r.$A) { $wB++ }
        else { $ties++ }
    }
    $n = $Rows.Count
    [pscustomobject]@{
        pair     = "$A vs $B"
        winsA    = $wA
        ties     = $ties
        winsB    = $wB
        rateA    = if ($n) { [math]::Round(100.0 * $wA / $n, 1) } else { 0 }
        rateB    = if ($n) { [math]::Round(100.0 * $wB / $n, 1) } else { 0 }
    }
}

$overall = Get-Summary -Rows $rows
$bySession = [ordered]@{}
foreach ($s in @($rows | Select-Object -ExpandProperty session -Unique | Sort-Object)) {
    $bySession[$s] = Get-Summary -Rows @($rows | Where-Object { $_.session -eq $s })
}
$pairs = @(
    (Get-Pair -Rows $rows -A "a" -B "b"),
    (Get-Pair -Rows $rows -A "a" -B "c"),
    (Get-Pair -Rows $rows -A "b" -B "c")
)

$out = [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
    rows           = $rows
    overall        = $overall
    bySession      = $bySession
    pairs          = $pairs
}
$outPath = Join-Path $reviewDir "blind-results.json"
$out | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $outPath -Encoding utf8

$md = @()
$md += "# Blind qualitative review results"
$md += ""
$md += "Generated: $($out.generatedAtUtc)"
$md += ""
$md += "## Overall ($($rows.Count) cases)"
$md += ""
$md += "| config | avg score | serious (score<=2) |"
$md += "|---|---|---|"
$md += "| a | $($overall.avgA) | $($overall.seriousA) |"
$md += "| b | $($overall.avgB) | $($overall.seriousB) |"
$md += "| c | $($overall.avgC) | $($overall.seriousC) |"
$md += ""
$md += "## Win / tie / loss"
$md += ""
$md += "| pair | A wins | ties | B wins | A win % | B win % |"
$md += "|---|---|---|---|---|---|"
foreach ($p in $pairs) {
    $md += "| $($p.pair) | $($p.winsA) | $($p.ties) | $($p.winsB) | $($p.rateA) | $($p.rateB) |"
}
$md += ""
$md += "## By session"
$md += ""
$md += "| session | a avg | b avg | c avg | a serious | b serious | c serious |"
$md += "|---|---|---|---|---|---|---|"
foreach ($s in $bySession.Keys) {
    $v = $bySession[$s]
    $md += "| $s | $($v.avgA) | $($v.avgB) | $($v.avgC) | $($v.seriousA) | $($v.seriousB) | $($v.seriousC) |"
}
$md += ""
$md | Set-Content -LiteralPath (Join-Path $reviewDir "blind-results.md") -Encoding utf8
Write-Host "wrote $outPath and blind-results.md"