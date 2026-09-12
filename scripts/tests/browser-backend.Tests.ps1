[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$startup = Join-Path $repositoryRoot "scripts\start-browser-backend.ps1"

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

. $startup

$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) (
    "meowcal-browser-backend-tests-" + [guid]::NewGuid().ToString("N")
)
try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $lockPath = Join-Path $temporaryDirectory "meowcal-core.lock.json"

    $withoutLock = Resolve-BrowserBackendPlan -LockPath $lockPath
    Assert-Equal $true $withoutLock.UseSourceCandidate "No lock uses a source candidate."
    Assert-Equal "build-source-candidate" $withoutLock.CorePreparation `
        "No lock builds the source candidate."
    Assert-Equal "--features core-source-candidate" ($withoutLock.CargoArguments -join " ") `
        "No lock enables the source candidate feature."

    Set-Content -LiteralPath $lockPath -Value "{}" -Encoding utf8
    $withLock = Resolve-BrowserBackendPlan -LockPath $lockPath
    Assert-Equal $false $withLock.UseSourceCandidate "A reviewed lock is the default browser backend."
    Assert-Equal "fetch-reviewed-release" $withLock.CorePreparation `
        "A reviewed lock fetches its release for the browser backend."
    Assert-Equal 0 $withLock.CargoArguments.Count "A reviewed lock does not enable the source candidate feature."

    $explicitCandidate = Resolve-BrowserBackendPlan -LockPath $lockPath -CoreSourceCandidate
    Assert-Equal $true $explicitCandidate.UseSourceCandidate "An explicit browser candidate overrides the lock."
    Assert-Equal "build-source-candidate" $explicitCandidate.CorePreparation `
        "An explicit browser candidate does not fetch the lock."
    Assert-Equal "--features core-source-candidate" ($explicitCandidate.CargoArguments -join " ") `
        "An explicit browser candidate enables its feature."

    $package = Get-Content -LiteralPath (Join-Path $repositoryRoot "package.json") -Raw | ConvertFrom-Json
    Assert-Equal "pwsh -NoProfile -File scripts/start-browser-backend.ps1" $package.scripts.'dev:backend' `
        "The default browser backend starts through the reviewed-release selector."
    Assert-Equal "pwsh -NoProfile -File scripts/start-browser-backend.ps1 -CoreSourceCandidate" `
        $package.scripts.'dev:backend:source-candidate' `
        "The source candidate browser backend remains explicit."

    $startupSource = Get-Content -LiteralPath $startup -Raw
    if ($startupSource -match "MEOWCAL_CORE_LOCK") {
        throw "The browser backend must use the same canonical Core lock path as build.rs."
    }

    Write-Host "Browser backend startup contract tests passed." -ForegroundColor Green
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
