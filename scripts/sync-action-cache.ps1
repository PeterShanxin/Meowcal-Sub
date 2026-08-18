<#
.SYNOPSIS
    Fill the self-hosted runner's action archive cache and point the runner at it.

.DESCRIPTION
    The runner deletes `_work\_actions` at the start of every job, so its own
    per-action watermark never survives and each job re-downloads
    `actions/checkout` from codeload.github.com. That download happens during job
    *initialization*, where no workflow-level retry can reach it, so a single bad
    response fails the job before a step exists. Issue #132 has the measurements;
    docs/SELF_HOSTED_RUNNERS.md has the operating model.

    This places the archives the workflows actually need under a cache directory
    the runner reads instead of the network, and records that directory in the
    runner's .env.

    It does NOT restart the runner. `.env` is read once, when the listener
    starts, and restarting a live listener can sever a dispatched job. This
    runner is started on demand, so the next start picks the setting up.

    Safe to run repeatedly: entries already present are re-verified, a corrupt
    one is replaced, and archives no longer named by any workflow are pruned.

.EXAMPLE
    .\scripts\sync-action-cache.ps1

.EXAMPLE
    .\scripts\sync-action-cache.ps1 -WhatIfOnly
#>
[CmdletBinding()]
param(
    # Kept in step with setup-self-hosted-runner.ps1's own default.
    [string]$RunnerDirectory = "C:\actions-runner\meowcal-sub",

    # Defaults to a sibling of the runner's _work directory. Deliberately not
    # inside _work: the runner wipes parts of that, and a clean checkout runs
    # `git clean -ffdx` over the workspace.
    [string]$CacheDirectory = "",

    # Report the plan and change nothing.
    [switch]$WhatIfOnly,

    # Leave archives no workflow names any more where they are.
    [switch]$NoPrune
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "action-cache.ps1")

if ([string]::IsNullOrWhiteSpace($CacheDirectory)) {
    $CacheDirectory = Join-Path $RunnerDirectory "action-archive-cache"
}

function Invoke-Node {
    param([Parameter(Mandatory)][string[]]$Arguments)

    $output = & node @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "node $($Arguments -join ' ') failed with exit code $LASTEXITCODE."
    }
    $output
}

function Invoke-GhJq {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Query
    )

    $value = & gh api $Path --jq $Query 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($value)) {
        throw "Could not read '$Path' from the GitHub API. Check 'gh auth status'."
    }
    ([string]$value).Trim()
}

function Resolve-ActionReference {
    <#
        Answers with the name and commit the runner itself would resolve.

        The repository is asked for its `full_name` rather than trusting the
        string in the workflow: a renamed repository still answers under its old
        name, and the runner files the archive under the name it resolved. The
        commit comes from the ref rather than the ref being used as a filename,
        so a moved tag misses the cache and re-downloads instead of serving stale
        bytes under a name that no longer means them.
    #>
    param([Parameter(Mandatory)][string]$Reference)

    $separator = $Reference.LastIndexOf("@")
    if ($separator -le 0) {
        throw "Not an action reference: '$Reference'."
    }
    $name = $Reference.Substring(0, $separator)
    $ref = $Reference.Substring($separator + 1)

    [pscustomobject]@{
        nameWithOwner = Invoke-GhJq -Path "repos/$name" -Query ".full_name"
        sha           = Invoke-GhJq -Path "repos/$name/commits/$ref" -Query ".sha"
    }
}

function Save-ActionArchive {
    param(
        [Parameter(Mandatory)][string]$Url,
        [Parameter(Mandatory)][string]$Destination
    )

    $directory = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $staging = "$Destination.download"

    try {
        Invoke-WebRequest -Uri $Url -OutFile $staging -MaximumRetryCount 5 -RetryIntervalSec 5 `
            -UseBasicParsing
        if (-not (Test-ActionArchive -Path $staging)) {
            throw "Downloaded archive from $Url is not a readable zip."
        }
        Move-Item -LiteralPath $staging -Destination $Destination -Force
    } finally {
        Remove-Item -LiteralPath $staging -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "==> Action archive cache" -ForegroundColor Cyan
Write-Host "Runner dir:   $RunnerDirectory"
Write-Host "Cache dir:    $CacheDirectory"

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    throw "Node.js is required to read the workflows. Install the version package.json declares."
}
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI not found. Install gh and run 'gh auth login'."
}

$planScript = Join-Path $repositoryRoot "scripts\build-action-cache-plan.mjs"
$references = @(Invoke-Node -Arguments @($planScript, "--refs") | Where-Object { $_ -ne "" })
Write-Host "References:   $($references -join ', ')"

$resolutions = @{}
foreach ($reference in $references) {
    $resolutions[$reference] = Resolve-ActionReference -Reference $reference
}

$resolutionFile = Join-Path ([System.IO.Path]::GetTempPath()) (
    "meowcal-action-cache-" + [guid]::NewGuid().ToString("N") + ".json")
try {
    $resolutions | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $resolutionFile -Encoding utf8
    $plan = @(Invoke-Node -Arguments @($planScript, "--plan", $resolutionFile) |
            Out-String | ConvertFrom-Json)
} finally {
    Remove-Item -LiteralPath $resolutionFile -Force -ErrorAction SilentlyContinue
}

if ($plan.Count -eq 0) {
    throw "The plan is empty. Refusing to touch the cache."
}

foreach ($entry in $plan) {
    Write-Host "  $($entry.ref) -> $($entry.relativePath)"
}

if ($WhatIfOnly) {
    Write-Host "-WhatIfOnly requested; nothing was downloaded, pruned, or written." -ForegroundColor Green
    return
}

if (-not (Test-Path -LiteralPath $RunnerDirectory -PathType Container)) {
    throw "No runner directory at $RunnerDirectory. Register one with setup-self-hosted-runner.ps1 -Mode Install first."
}

New-Item -ItemType Directory -Path $CacheDirectory -Force | Out-Null

foreach ($entry in $plan) {
    $destination = Join-Path $CacheDirectory $entry.relativePath

    if (Test-Path -LiteralPath $destination -PathType Leaf) {
        if (Test-ActionArchive -Path $destination) {
            Write-Host "present|$($entry.relativePath)"
            continue
        }
        # Repair is the same code as first fetch: deleting here and falling
        # through means a corrupt entry can never become permanent by being
        # reported as present on every later run.
        Write-Host "corrupt-cached|$($entry.relativePath)" -ForegroundColor Yellow
        Remove-Item -LiteralPath $destination -Force
    }

    Save-ActionArchive -Url $entry.url -Destination $destination
    Write-Host "cached|$($entry.relativePath)"
}

if (-not $NoPrune) {
    $present = @(Get-ChildItem -LiteralPath $CacheDirectory -Recurse -File -Filter "*.zip" |
            ForEach-Object { $_.FullName.Substring($CacheDirectory.Length).TrimStart("\") })
    $stalePaths = @(Get-StaleActionCacheFiles -PresentRelativePaths $present `
            -PlannedRelativePaths @($plan.relativePath))
    foreach ($stale in $stalePaths) {
        Remove-Item -LiteralPath (Join-Path $CacheDirectory $stale) -Force
        Write-Host "pruned|$stale"
    }
}

$environmentFile = Join-Path $RunnerDirectory ".env"
$existing = if (Test-Path -LiteralPath $environmentFile -PathType Leaf) {
    @(Get-Content -LiteralPath $environmentFile)
} else {
    @()
}
$wanted = Get-ActionCacheEnvLine -CacheDirectory $CacheDirectory

if ($existing -contains $wanted) {
    Write-Host "env|unchanged"
} else {
    $updated = @(Set-ActionCacheEnvContent -ExistingLines $existing -CacheDirectory $CacheDirectory)
    Set-Content -LiteralPath $environmentFile -Value $updated -Encoding utf8

    # What matters is not that the write returned, but that the line is in the
    # file afterwards - a partial write differs from a failed one.
    if (@(Get-Content -LiteralPath $environmentFile) -notcontains $wanted) {
        throw "Wrote $environmentFile but it does not contain '$wanted'."
    }
    Write-Host "env|updated"
    Write-Host "The runner reads .env once at listener start; this engages at the next start." `
        -ForegroundColor Yellow
}

Write-Host "Action archive cache ready." -ForegroundColor Green
