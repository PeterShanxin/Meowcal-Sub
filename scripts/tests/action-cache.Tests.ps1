[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$scriptsDirectory = Split-Path -Parent $PSScriptRoot
. (Join-Path $scriptsDirectory "action-cache.ps1")

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Message)

    if (-not $Condition) {
        throw $Message
    }
}

$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) (
    "meowcal-action-cache-tests-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null

try {
    # --- The .env line ------------------------------------------------------

    Assert-Equal "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE" (Get-ActionCacheEnvName) `
        "The variable the runner reads is fixed by the runner, not by us."

    Assert-Equal "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=C:\cache" `
    (Get-ActionCacheEnvLine -CacheDirectory "C:\cache") `
        "The .env line is the variable, an equals sign, and the directory."

    # --- Composing the runner's .env ---------------------------------------

    $fromNothing = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache")
    Assert-Equal 1 $fromNothing.Count "A missing .env becomes a one-line file."
    Assert-Equal "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=C:\cache" $fromNothing[0] `
        "That one line is the setting."

    # The runner's .env is shared. A future setting this repository does not own
    # must survive a cache refresh, so the file is edited rather than rewritten.
    $withOthers = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache" -ExistingLines @(
            "SOMETHING_ELSE=1"))
    Assert-Equal 2 $withOthers.Count "Unrelated settings survive."
    Assert-Equal "SOMETHING_ELSE=1" $withOthers[0] "Unrelated settings keep their value."

    # Two assignments of the same key would make the effective value depend on
    # order, so an existing one is replaced however it was spaced.
    $replaced = @(Set-ActionCacheEnvContent -CacheDirectory "C:\new" -ExistingLines @(
            "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=C:\old",
            "  ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE = C:\older",
            "KEEP=yes"))
    Assert-Equal 2 $replaced.Count "Every previous assignment is replaced, not appended to."
    Assert-Equal "KEEP=yes" $replaced[0] "Replacing ours leaves other lines alone."
    Assert-Equal "ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=C:\new" $replaced[1] "The new value wins."

    # A line that merely mentions the variable is not an assignment of it.
    $mention = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache" -ExistingLines @(
            "NOTE_ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=documented elsewhere"))
    Assert-Equal 2 $mention.Count "A different key that contains our name is left alone."

    # Idempotence matters because this runs before every runner start.
    $once = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache" -ExistingLines @("KEEP=yes"))
    $twice = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache" -ExistingLines $once)
    Assert-Equal ($once -join "`n") ($twice -join "`n") "Re-applying the setting changes nothing."

    $padded = @(Set-ActionCacheEnvContent -CacheDirectory "C:\cache" -ExistingLines @(
            "KEEP=yes", "", ""))
    Assert-Equal 2 $padded.Count "Trailing blank lines do not accumulate one per refresh."

    # --- Verifying archive bytes -------------------------------------------

    $payload = Join-Path $temporaryDirectory "payload.txt"
    Set-Content -LiteralPath $payload -Value "action" -Encoding utf8
    $goodArchive = Join-Path $temporaryDirectory "good.zip"
    Compress-Archive -LiteralPath $payload -DestinationPath $goodArchive -Force

    Assert-True (Test-ActionArchive -Path $goodArchive) "A real zip verifies."

    # This is the shape that makes a size check useless: a rate-limit or redirect
    # body is a perfectly good file of plausible length.
    $ratelimitPage = Join-Path $temporaryDirectory "429.zip"
    Set-Content -LiteralPath $ratelimitPage -Value "You have exceeded a secondary rate limit." `
        -Encoding utf8
    Assert-True (-not (Test-ActionArchive -Path $ratelimitPage)) `
        "A plausible non-zip body is refused."

    $truncated = Join-Path $temporaryDirectory "truncated.zip"
    $bytes = [System.IO.File]::ReadAllBytes($goodArchive)
    [System.IO.File]::WriteAllBytes($truncated, $bytes[0..([int]($bytes.Length / 2))])
    Assert-True (-not (Test-ActionArchive -Path $truncated)) "A half-written archive is refused."

    $empty = Join-Path $temporaryDirectory "empty.zip"
    New-Item -ItemType File -Path $empty | Out-Null
    Assert-True (-not (Test-ActionArchive -Path $empty)) "A zero-length file is refused."

    Assert-True (-not (Test-ActionArchive -Path (Join-Path $temporaryDirectory "absent.zip"))) `
        "A file that is not there is not a cache hit."

    # --- Pruning ------------------------------------------------------------

    $stale = @(Get-StaleActionCacheFiles `
            -PresentRelativePaths @("actions_checkout\aaa.zip", "actions_checkout\bbb.zip") `
            -PlannedRelativePaths @("actions_checkout\bbb.zip"))
    Assert-Equal 1 $stale.Count "Only archives no workflow names any more are stale."
    Assert-Equal "actions_checkout\aaa.zip" $stale[0] "The superseded archive is the stale one."

    $casing = @(Get-StaleActionCacheFiles `
            -PresentRelativePaths @("Actions_Checkout\AAA.zip") `
            -PlannedRelativePaths @("actions_checkout\aaa.zip"))
    Assert-Equal 0 $casing.Count "These are Windows paths; case is not a difference."

    # "nothing is planned" and "everything is stale" are the same set, and only
    # one of them is a reason to delete a working cache.
    $refused = $false
    try {
        Get-StaleActionCacheFiles -PresentRelativePaths @("actions_checkout\aaa.zip") `
            -PlannedRelativePaths @()
    } catch {
        $refused = $true
    }
    Assert-True $refused "An empty plan refuses to declare the whole cache stale."

    # --- Lifecycle contract -------------------------------------------------

    # Refreshing a cache must not be able to take the runner offline. The
    # setting is read once at listener start, and this runner is started on
    # demand, so waiting costs one session of uncached runs while restarting a
    # live listener can sever a dispatched job.
    $syncSource = Get-Content -LiteralPath (Join-Path $scriptsDirectory "sync-action-cache.ps1") `
        -Raw
    $syncBody = [regex]::Replace($syncSource, "(?s)<#.*?#>", "")
    foreach ($forbidden in @("Stop-Process", "Restart-Service", "Stop-Service", "run.cmd")) {
        Assert-True ($syncBody -notmatch [regex]::Escape($forbidden)) `
            "sync-action-cache.ps1 must not touch runner lifecycle state ($forbidden)."
    }

    # The runner start path must survive a cache refresh it could not complete:
    # an unreachable GitHub is a slower CI run, never a runner that will not
    # start.
    $startSource = Get-Content -LiteralPath (
        Join-Path $scriptsDirectory "setup-self-hosted-runner.ps1") -Raw
    Assert-True ($startSource -match "sync-action-cache\.ps1") `
        "setup-self-hosted-runner.ps1 must refresh the action archive cache before starting."
    Assert-True ($startSource -match "(?s)try\s*\{[^}]*sync-action-cache\.ps1.*?\}\s*catch") `
        "The cache refresh must be best effort, so a failed one cannot block a start."

    Write-Host "action cache contract tests passed."
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
