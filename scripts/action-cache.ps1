<#
.SYNOPSIS
    Pure helpers for the runner's action archive cache.

.DESCRIPTION
    Dot-sourced by scripts/sync-action-cache.ps1 and exercised directly by
    scripts/tests/action-cache.Tests.ps1. Nothing here reaches the network, so
    every rule that decides what lands in the cache, what is removed from it, and
    what the runner's .env ends up saying can be tested on any machine.

    See docs/SELF_HOSTED_RUNNERS.md for the operating model and issue #132 for
    the failure this exists to prevent.
#>

# The runner reads this once, in Program.LoadAndSetEnv, when the listener starts.
$script:ActionCacheEnvName = "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE"

function Get-ActionCacheEnvName {
    $script:ActionCacheEnvName
}

function Get-ActionCacheEnvLine {
    <#
        The exact line the runner's .env must contain. One place, so the writer
        and the read-back check cannot drift apart and report a success the file
        does not support.
    #>
    param([Parameter(Mandatory)][string]$CacheDirectory)

    "$(Get-ActionCacheEnvName)=$CacheDirectory"
}

function Set-ActionCacheEnvContent {
    <#
        Returns what the runner's .env should contain, given what it contains
        now.

        Rewriting the file wholesale would be shorter and would discard settings
        this repository does not own; the runner's .env is a shared file and a
        future setting must survive a cache refresh. Any existing assignment of
        our own variable is replaced rather than appended to, because the runner
        keeps the last assignment and two lines would make the effective value
        depend on order.
    #>
    param(
        [string[]]$ExistingLines = @(),
        [Parameter(Mandatory)][string]$CacheDirectory
    )

    $name = Get-ActionCacheEnvName
    $wanted = Get-ActionCacheEnvLine -CacheDirectory $CacheDirectory

    $kept = @()
    foreach ($line in $ExistingLines) {
        if ($null -eq $line) { continue }
        # Match the assignment, not the mention: a line whose key is ours is
        # replaced whatever its spacing, and anything else is left alone.
        if ($line -match "^\s*$([regex]::Escape($name))\s*=") { continue }
        $kept += $line
    }

    # Trailing blank lines would accumulate one per refresh.
    while ($kept.Count -gt 0 -and [string]::IsNullOrWhiteSpace($kept[-1])) {
        $kept = $kept[0..($kept.Count - 2)]
    }

    # Callers wrap this in @(). A one-line result would otherwise arrive as a
    # bare string and index into its characters.
    $kept + $wanted
}

function Test-ActionArchive {
    <#
        True when the file is a readable zip with at least one entry.

        A size check is not enough and the difference is the whole point: a
        rate-limit page, a redirect body, or a truncated response is a perfectly
        good file of plausible length. A corrupt archive in the cache fails
        *every* job, which is worse than the download it replaces, so this runs
        on entries already present as well as on new ones.
    #>
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    if ((Get-Item -LiteralPath $Path).Length -le 0) { return $false }

    $archive = $null
    try {
        $archive = [System.IO.Compression.ZipFile]::OpenRead($Path)
        return $archive.Entries.Count -gt 0
    } catch {
        return $false
    } finally {
        if ($archive) { $archive.Dispose() }
    }
}

function Get-StaleActionCacheFiles {
    <#
        The cached archives no longer named by the plan, as relative paths.

        Comparison is case-insensitive because these are Windows paths, and the
        caller must refuse an empty plan before calling: "nothing is planned"
        and "everything is stale" are the same set, and only one of them is a
        reason to delete a working cache.
    #>
    param(
        [string[]]$PresentRelativePaths = @(),
        [string[]]$PlannedRelativePaths = @()
    )

    if ($PlannedRelativePaths.Count -eq 0) {
        throw "Refusing to compute stale archives from an empty plan."
    }

    $planned = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]$PlannedRelativePaths, [System.StringComparer]::OrdinalIgnoreCase)

    # Callers wrap this in @(); see Set-ActionCacheEnvContent.
    $PresentRelativePaths | Where-Object { -not $planned.Contains($_) }
}
