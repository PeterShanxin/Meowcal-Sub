[CmdletBinding()]
param(
    [switch]$CoreSourceCandidate
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

function Resolve-BrowserBackendPlan {
    param(
        [Parameter(Mandatory)][string]$LockPath,
        [switch]$CoreSourceCandidate
    )

    if ($CoreSourceCandidate -or -not (Test-Path -LiteralPath $LockPath -PathType Leaf)) {
        return [pscustomobject]@{
            UseSourceCandidate = $true
            CorePreparation = "build-source-candidate"
            CargoArguments = @("--features", "core-source-candidate")
        }
    }

    [pscustomobject]@{
        UseSourceCandidate = $false
        CorePreparation = "fetch-reviewed-release"
        CargoArguments = @()
    }
}

if ($MyInvocation.InvocationName -eq ".") { return }

$lockPath = Join-Path $repositoryRoot "config\meowcal-core.lock.json"
$plan = Resolve-BrowserBackendPlan -LockPath $lockPath -CoreSourceCandidate:$CoreSourceCandidate

& (Join-Path $PSScriptRoot "prepare-validation-resources.ps1")
if ($plan.CorePreparation -eq "build-source-candidate") {
    Write-Host "Using source-built Core candidate for the browser backend." -ForegroundColor Yellow
    & (Join-Path $PSScriptRoot "prepare-core-resource.ps1") -Configuration Release | Out-Null
} else {
    Write-Host "Fetching reviewed Core release for the browser backend." -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot "fetch-meowcal-core.ps1") -LockPath $lockPath | Out-Null
}
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Push-Location (Join-Path $repositoryRoot "src-tauri")
try {
    & cargo run --locked @($plan.CargoArguments) -- --http-only
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
