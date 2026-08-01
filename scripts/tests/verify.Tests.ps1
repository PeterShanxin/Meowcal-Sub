[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$verifyScript = Join-Path $repositoryRoot "scripts\verify.ps1"
$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) (
    "meowcal-verify-tests-" + [guid]::NewGuid().ToString("N")
)
$cargoShim = Join-Path $temporaryDirectory "cargo.cmd"
$cargoLog = Join-Path $temporaryDirectory "cargo.log"
$npmShim = Join-Path $temporaryDirectory "npm.cmd"
$npmLog = Join-Path $temporaryDirectory "npm.log"

function Assert-Equal {
    param(
        $Expected,
        $Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-Lines {
    param(
        [string[]]$Expected,
        [string[]]$Actual,
        [string]$Message
    )

    Assert-Equal $Expected.Count $Actual.Count "$Message command count."
    for ($index = 0; $index -lt $Expected.Count; $index++) {
        Assert-Equal $Expected[$index] $Actual[$index] "$Message command $index."
    }
}

function Invoke-VerifyUnderTest {
    param(
        [ValidateSet("All", "Lint", "Test", "Frontend")]
        [string]$Stage,
        [string]$CargoFailOn = "",
        [string]$NpmFailOn = ""
    )

    Remove-Item -LiteralPath $cargoLog -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $npmLog -Force -ErrorAction SilentlyContinue
    $previousPath = $env:PATH
    $previousCargoLog = $env:FAKE_CARGO_LOG
    $previousCargoFailOn = $env:FAKE_CARGO_FAIL_ON
    $previousNpmLog = $env:FAKE_NPM_LOG
    $previousNpmFailOn = $env:FAKE_NPM_FAIL_ON
    $previousContractState = $env:MEOWCAL_VERIFY_CONTRACT_ACTIVE

    try {
        $env:PATH = "$temporaryDirectory;$previousPath"
        $env:FAKE_CARGO_LOG = $cargoLog
        $env:FAKE_CARGO_FAIL_ON = $CargoFailOn
        $env:FAKE_NPM_LOG = $npmLog
        $env:FAKE_NPM_FAIL_ON = $NpmFailOn
        $env:MEOWCAL_VERIFY_CONTRACT_ACTIVE = "1"
        & pwsh -NoProfile -File $verifyScript -Stage $Stage
        $exitCode = $LASTEXITCODE
        $cargoCommands = if (Test-Path -LiteralPath $cargoLog) {
            @(Get-Content -LiteralPath $cargoLog)
        } else {
            @()
        }
        $npmCommands = if (Test-Path -LiteralPath $npmLog) {
            @(Get-Content -LiteralPath $npmLog)
        } else {
            @()
        }

        return [pscustomobject]@{
            ExitCode = $exitCode
            CargoCommands = $cargoCommands
            NpmCommands = $npmCommands
        }
    } finally {
        $env:PATH = $previousPath
        $env:FAKE_CARGO_LOG = $previousCargoLog
        $env:FAKE_CARGO_FAIL_ON = $previousCargoFailOn
        $env:FAKE_NPM_LOG = $previousNpmLog
        $env:FAKE_NPM_FAIL_ON = $previousNpmFailOn
        $env:MEOWCAL_VERIFY_CONTRACT_ACTIVE = $previousContractState
    }
}

New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null

try {
    @'
@echo off
echo %*>>"%FAKE_CARGO_LOG%"
if "%FAKE_CARGO_FAIL_ON%"=="" exit /b 0
echo %* | findstr /C:"%FAKE_CARGO_FAIL_ON%" >nul
if not errorlevel 1 exit /b 23
exit /b 0
'@ | Set-Content -LiteralPath $cargoShim -Encoding ascii

    @'
@echo off
echo %*>>"%FAKE_NPM_LOG%"
if "%FAKE_NPM_FAIL_ON%"=="" exit /b 0
echo %* | findstr /C:"%FAKE_NPM_FAIL_ON%" >nul
if not errorlevel 1 exit /b 29
exit /b 0
'@ | Set-Content -LiteralPath $npmShim -Encoding ascii

    $lint = Invoke-VerifyUnderTest -Stage Lint
    Assert-Equal 0 $lint.ExitCode "Lint stage exit code."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings"
    ) $lint.CargoCommands "Lint stage"
    Assert-Lines @() $lint.NpmCommands "Lint stage npm"

    $test = Invoke-VerifyUnderTest -Stage Test
    Assert-Equal 0 $test.ExitCode "Test stage exit code."
    Assert-Lines @(
        "test --locked --lib",
        "test --locked --test integration_ipc"
    ) $test.CargoCommands "Test stage"
    Assert-Lines @() $test.NpmCommands "Test stage npm"

    $frontend = Invoke-VerifyUnderTest -Stage Frontend
    Assert-Equal 0 $frontend.ExitCode "Frontend stage exit code."
    Assert-Lines @() $frontend.CargoCommands "Frontend stage cargo"
    Assert-Lines @(
        "ci --ignore-scripts",
        "run format:check",
        "run lint",
        "run typecheck",
        "run build:web",
        "run maintainability",
        "run test:frontend",
        "run test:browser",
        "audit --audit-level=high"
    ) $frontend.NpmCommands "Frontend stage"

    $all = Invoke-VerifyUnderTest -Stage All
    Assert-Equal 0 $all.ExitCode "All stage exit code."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings",
        "test --locked --lib",
        "test --locked --test integration_ipc"
    ) $all.CargoCommands "All stage cargo"
    Assert-Lines @(
        "ci --ignore-scripts",
        "run format:check",
        "run lint",
        "run typecheck",
        "run build:web",
        "run maintainability",
        "run test:frontend",
        "run test:browser",
        "audit --audit-level=high"
    ) $all.NpmCommands "All stage npm"

    $failure = Invoke-VerifyUnderTest -Stage All -CargoFailOn "clippy"
    Assert-Equal 23 $failure.ExitCode "Failure propagation."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings"
    ) $failure.CargoCommands "Failure short-circuit"
    Assert-Lines @() $failure.NpmCommands "Cargo failure prevents npm"

    $npmFailure = Invoke-VerifyUnderTest -Stage Frontend -NpmFailOn "run lint"
    Assert-Equal 29 $npmFailure.ExitCode "Npm failure propagation."
    Assert-Lines @(
        "ci --ignore-scripts",
        "run format:check",
        "run lint"
    ) $npmFailure.NpmCommands "Npm failure short-circuit"

    Write-Host "verify.ps1 contract tests passed."
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
