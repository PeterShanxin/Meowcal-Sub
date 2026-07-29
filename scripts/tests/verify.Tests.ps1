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
        [ValidateSet("All", "Lint", "Test")]
        [string]$Stage,
        [string]$FailOn = ""
    )

    Remove-Item -LiteralPath $cargoLog -Force -ErrorAction SilentlyContinue
    $previousPath = $env:PATH
    $previousLog = $env:FAKE_CARGO_LOG
    $previousFailOn = $env:FAKE_CARGO_FAIL_ON
    $previousContractState = $env:MEOWCAL_VERIFY_CONTRACT_ACTIVE

    try {
        $env:PATH = "$temporaryDirectory;$previousPath"
        $env:FAKE_CARGO_LOG = $cargoLog
        $env:FAKE_CARGO_FAIL_ON = $FailOn
        $env:MEOWCAL_VERIFY_CONTRACT_ACTIVE = "1"
        & pwsh -NoProfile -File $verifyScript -Stage $Stage
        $exitCode = $LASTEXITCODE
        $commands = if (Test-Path -LiteralPath $cargoLog) {
            @(Get-Content -LiteralPath $cargoLog)
        } else {
            @()
        }

        return [pscustomobject]@{
            ExitCode = $exitCode
            Commands = $commands
        }
    } finally {
        $env:PATH = $previousPath
        $env:FAKE_CARGO_LOG = $previousLog
        $env:FAKE_CARGO_FAIL_ON = $previousFailOn
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

    $lint = Invoke-VerifyUnderTest -Stage Lint
    Assert-Equal 0 $lint.ExitCode "Lint stage exit code."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings"
    ) $lint.Commands "Lint stage"

    $test = Invoke-VerifyUnderTest -Stage Test
    Assert-Equal 0 $test.ExitCode "Test stage exit code."
    Assert-Lines @(
        "test --locked --lib",
        "test --locked --test integration_ipc"
    ) $test.Commands "Test stage"

    $all = Invoke-VerifyUnderTest -Stage All
    Assert-Equal 0 $all.ExitCode "All stage exit code."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings",
        "test --locked --lib",
        "test --locked --test integration_ipc"
    ) $all.Commands "All stage"

    $failure = Invoke-VerifyUnderTest -Stage All -FailOn "clippy"
    Assert-Equal 23 $failure.ExitCode "Failure propagation."
    Assert-Lines @(
        "fmt --check",
        "clippy --locked -- -D warnings"
    ) $failure.Commands "Failure short-circuit"

    Write-Host "verify.ps1 contract tests passed."
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
