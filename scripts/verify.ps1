[CmdletBinding()]
param(
    [ValidateSet("All", "Lint", "Test")]
    [string]$Stage = "All"
)

$ErrorActionPreference = "Stop"
$env:CARGO_TERM_COLOR = "always"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$resourceScript = Join-Path $PSScriptRoot "prepare-validation-resources.ps1"
$contractTest = Join-Path $PSScriptRoot "tests\verify.Tests.ps1"
$rustDirectory = Join-Path $repositoryRoot "src-tauri"

function Invoke-CargoStep {
    param(
        [string]$Name,
        [string[]]$Arguments
    )

    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & cargo @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        Write-Host "$Name failed with exit code $exitCode." -ForegroundColor Red
        exit $exitCode
    }
}

if ($env:MEOWCAL_VERIFY_CONTRACT_ACTIVE -ne "1") {
    Write-Host "==> Verification contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $contractTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

& $resourceScript

Push-Location $rustDirectory
try {
    if ($Stage -in @("All", "Lint")) {
        Invoke-CargoStep "Rust format" @("fmt", "--check")
        Invoke-CargoStep "Rust clippy" @("clippy", "--locked", "--", "-D", "warnings")
    }

    if ($Stage -in @("All", "Test")) {
        Invoke-CargoStep "Rust unit tests" @("test", "--locked", "--lib")
        Invoke-CargoStep "Rust IPC integration tests" @(
            "test",
            "--locked",
            "--test",
            "integration_ipc"
        )
    }
} finally {
    Pop-Location
}

Write-Host ""
Write-Host "Verification stage '$Stage' passed." -ForegroundColor Green
