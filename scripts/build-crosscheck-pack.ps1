# =============================================================================
# BUILD-CROSSCHECK-PACK.PS1 - historical-reference-free blind cross-check pack
# =============================================================================
# Recomputes a compact (~50 case) blind pack purely from existing benchmark
# artifacts. NO inference, NO model startup, NO reruns, no writes outside
# <BenchDir>\review\crosscheck-*.
#
# Blindness fixes vs. the original review:
#   * historical translation is NEVER shown in the pack
#   * per-case X/Y/Z permutation is FRESHLY generated (seed-driven RNG), never
#     the old blind-key permutation
#   * pack cells expose only anonymized ids + source + X/Y/Z candidate text;
#     real case id, session, config<->label permutation, previous scores, kind
#     and old notes are ONLY in crosscheck-key.json
#
# Sampling (deterministic from Seed, objective, no manual picks):
#   1. high-disagreement (TargetHigh, meaningful only): rank ALL meaningful
#      cases purely from objective previous decoded scores:
#          spread = max(a,b,c)-min(a,b,c)  (desc)
#          min    = min(a,b,c)             (asc; serious wins ties)
#      with a seeded rng tie-break. No "one model beat another" reasoning.
#   2. general (TargetGeneral, meaningful): remaining meaningful, seeded rng,
#      distributed evenly across all sessions (>=1 per session until target).
#   3. noise (TargetNoise): seeded rng among the OCR/noise-classified cases.
# No duplicate cases across pools.
#
# Usage: .\scripts\build-crosscheck-pack.ps1 [-Seed 20260809]
#          [-TargetGeneral 30] [-TargetHigh 10] [-TargetNoise 10] [-BenchDir D]
# Writes under <BenchDir>\review:
#   crosscheck-pack.json      - what the fresh reviewer sees
#   crosscheck-key.json       - decode key (kept separate)
# =============================================================================

[CmdletBinding()]
param(
    [int]$Seed          = 20260809,
    [int]$TargetGeneral = 30,
    [int]$TargetHigh    = 10,
    [int]$TargetNoise   = 10,
    [string]$BenchDir   = ""
)

$ErrorActionPreference = "Stop"
if (-not $BenchDir) { $BenchDir = Join-Path (Split-Path -Parent $PSScriptRoot) "eval-results\bench" }
$reviewDir = Join-Path $BenchDir "review"

# ---------------------------------------------------------------- sources ---
$packJson = @(Get-Content -Raw -LiteralPath (Join-Path $reviewDir "blind-pack.json")  | ConvertFrom-Json)
$keyJson  = Get-Content -Raw -LiteralPath (Join-Path $reviewDir "blind-key.json")     | ConvertFrom-Json
$resJson  = Get-Content -Raw -LiteralPath (Join-Path $reviewDir "blind-results.json") | ConvertFrom-Json

$classMap = @{}
Get-ChildItem -LiteralPath (Join-Path $reviewDir "blind-scores") -Filter "src-class-*.json" | ForEach-Object {
    foreach ($e in @(Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json)) { $classMap[$e.caseId] = $e.kind }
}

$oldKey = @{}
foreach ($prop in $keyJson.PSObject.Properties) { $oldKey[$prop.Name] = $prop.Value }

$prev = @{}
foreach ($row in @($resJson.rows)) { $prev[$row.caseId] = $row }

# ------------------------------------------------------------------ enrich ---
$record = [ordered]@{}
foreach ($p in $packJson) {
    $cid = [string]$p.caseId
    $lblOf = $oldKey[$cid].labelOf
    $outCfg = [ordered]@{}
    foreach ($lab in @('X', 'Y', 'Z')) {
        $cfg   = $lblOf.$lab
        $outCfg[$cfg] = if ($null -ne $p.labels.$lab) { [string]$p.labels.$lab } else { $null }
    }
    $row = $prev[$cid]
    $vv  = @([int]$row.a, [int]$row.b, [int]$row.c)
    $record[$cid] = [pscustomobject]@{
        caseId   = $cid
        session  = [string]$p.session
        kind     = $classMap[$cid]
        source   = [string]$p.source
        out      = [ordered]@{ a = $outCfg.a; b = $outCfg.b; c = $outCfg.c }
        scA      = $vv[0]
        scB      = $vv[1]
        scC      = $vv[2]
        spread   = (($vv | Measure-Object -Maximum).Maximum) - (($vv | Measure-Object -Minimum).Minimum)
        minscore = ($vv | Measure-Object -Minimum).Minimum
        note     = if ($null -ne $row.note) { [string]$row.note } else { $null }
    }
}

# -------------------------------------------------------------------- rng ----
$rng = [System.Random]::new($Seed)
function Get-Shuffle {
    param([object[]]$Items)
    $a = @($Items)
    for ($i = $a.Count - 1; $i -gt 0; $i--) {
        $j = $rng.Next(0, $i + 1)
        $t = $a[$i]; $a[$i] = $a[$j]; $a[$j] = $t
    }
    return $a
}
function New-Perm {
    # fresh mapping: label -> config for X, Y, Z
    $perm = @(Get-Shuffle @('a', 'b', 'c'))
    return [ordered]@{ X = $perm[0]; Y = $perm[1]; Z = $perm[2] }
}

# ------------------------------------------------------------ session list ---
$sessions = @($record.Values | Select-Object -ExpandProperty session -Unique | Sort-Object)
$sessionCount = $sessions.Count

# ------------------------------------------------------------- selection ----
$selection = [ordered]@{ }     # caseId -> [ordered]@{ reason=..; order=.. }
$nextOrder = [int]1
function Add-Pick {
    param([string]$CaseId, [string]$Pipe)
    if ($selection.Contains($CaseId)) { return }
    $order = $script:nextOrder
    $script:nextOrder += 1
    $selection[$CaseId] = [ordered]@{ reason = $Pipe; order = $order }
}

$meaningful = @($record.Values | Where-Object { $_.kind -eq 'meaningful' })
$noise      = @($record.Values | Where-Object { $_.kind -eq 'noise' })

function Count-Reason($Reason) { return @($selection.Values | Where-Object { $_.reason -eq $Reason }).Count }

# -- 1) high-disagreement: objective ranking ----------------------------------
$ranked = @($meaningful | Sort-Object -Property @{e = { $_.spread }; Descending = $true },
                                                  @{e = { $_.minscore }; Descending = $false },
                                                  @{e = { $rng.Next() }; Descending = $false })
foreach ($r in $ranked) {
    if ((Count-Reason 'high') -ge $TargetHigh) { break }
    Add-Pick $r.caseId 'high'
}

# -- 2) general: evenly across sessions ----------------------------------------
$generalPool = @($meaningful | Where-Object { -not $selection.Contains($_.caseId) })
$bySession = @{}
foreach ($r in $generalPool) {
    if (-not $bySession.ContainsKey($r.session)) { $bySession[$r.session] = @() }
    $bySession[$r.session] += $r.caseId
}
$sessionOrder = @(Get-Shuffle $sessions)
$perSessionCap = [math]::Ceiling($TargetGeneral / $sessionCount)
$pickedPerSession = @{}
foreach ($tag in $sessions) { $pickedPerSession[$tag] = 0 }

foreach ($tag in @(Get-Shuffle $sessions)) {
    if ((Count-Reason 'general') -ge $TargetGeneral) { break }
    $pool = @(Get-Shuffle $bySession[$tag])
    foreach ($cid in $pool) {
        if ((Count-Reason 'general') -ge $TargetGeneral) { break }
        if ($selection.Contains($cid)) { continue }
        if ($pickedPerSession[$tag] -ge $perSessionCap) { break }
        Add-Pick $cid 'general'
        $pickedPerSession[$tag] = $pickedPerSession[$tag] + 1
    }
}

# -- 3) noise ---------------------------------------------------------------
$noisePool = @(Get-Shuffle ($noise | ForEach-Object { $_.caseId }))
foreach ($cid in $noisePool) {
    if ((Count-Reason 'noise') -ge $TargetNoise) { break }
    Add-Pick $cid 'noise'
}

# ---------------------------------------------------------------- assemble ----
$serial = 0
$cells  = @()
$keyMap = [ordered]@{}

foreach ($cid in @($selection.Keys | Sort-Object { $selection[$_].order })) {
    $rec = $record[$cid]
    $perm = New-Perm
    $serial = $serial + 1
    $anon = 'CC-{0:D3}' -f $serial

    $cells += [ordered]@{
        id     = $anon
        source = $rec.source
        X      = if ($rec.out.$($perm.X)) { [string]$rec.out.$($perm.X) } else { '' }
        Y      = if ($rec.out.$($perm.Y)) { [string]$rec.out.$($perm.Y) } else { '' }
        Z      = if ($rec.out.$($perm.Z)) { [string]$rec.out.$($perm.Z) } else { '' }
    }

    $keyMap[$anon] = [ordered]@{
        anonId     = $anon
        caseId     = $rec.caseId
        session    = $rec.session
        pool       = $selection[$cid].reason
        sourceKind = $rec.kind
        perm       = $perm
        previous   = [ordered]@{ a = $rec.scA; b = $rec.scB; c = $rec.scC }
        note       = $rec.note
    }
}

$nHigh  = Count-Reason 'high'
$nGen   = Count-Reason 'general'
$nNoise = Count-Reason 'noise'
Write-Host "selected  high=$nHigh general=$nGen noise=$nNoise total=$($selection.Count)"
if ($nHigh  -ne $TargetHigh)    { Write-Warning "high target $TargetHigh != $nHigh" }
if ($nGen   -ne $TargetGeneral) { Write-Warning "general target $TargetGeneral != $nGen" }
if ($nNoise -ne $TargetNoise)   { Write-Warning "noise target $TargetNoise != $nNoise" }

$sessionsSeen = @($selection.Keys | ForEach-Object { $record[$_].session } | Sort-Object -Unique)
Write-Host "sessions represented: $($sessionsSeen -join '; ')"

# ---------------------------------------------------------------- writing ----
$outPack = [ordered]@{
    schema         = 'meowcal-sub/crosscheck-pack/v1'
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    seed           = $Seed
    note           = 'Judge only against the source text. Do not assume any candidate X/Y/Z is the historical or production translation.'
    count          = $cells.Count
    cells          = $cells
}
$outPack | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $reviewDir 'crosscheck-pack.json') -Encoding utf8

$outKey = [ordered]@{
    schemaVersion = 'meowcal-sub/crosscheck-key/v1'
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    seed          = $Seed
    note          = 'Keep separate from the pack. Maps anonymized ids to original case data, label permutations, and previous decoded scores.'
    cases         = $keyMap
}
$outKey | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $reviewDir 'crosscheck-key.json') -Encoding utf8

Write-Host "wrote $(Join-Path $reviewDir 'crosscheck-pack.json') and $(Join-Path $reviewDir 'crosscheck-key.json')"