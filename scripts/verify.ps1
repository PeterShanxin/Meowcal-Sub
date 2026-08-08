[CmdletBinding()]
param(
    [ValidateSet("All", "Lint", "Test", "Frontend")]
    [string]$Stage = "All",

    # "host" is the historical behavior: cargo picks the host triple and no
    # --target flag is passed. Naming a triple is what lets one machine cover
    # both shipped architectures, because the crate compiles genuinely different
    # code for each - see the cfg(target_arch) split in `engine_launch.rs`.
    [ValidateSet("host", "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc")]
    [string]$Target = "host"
)

$ErrorActionPreference = "Stop"
$env:CARGO_TERM_COLOR = "always"

# Read the machine, not the process. PROCESSOR_ARCHITECTURE reports AMD64 to an
# emulated x64 shell running on an ARM64 machine, which would silently disable
# both guards below.
$hostIsArm64 =
    [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64"

# Windows x64 emulation runs one way only. An ARM64 host executes both shipped
# architectures; an x64 host executes only its own. Refusing here explains the
# problem, where letting it through fails later as an unreadable exec error.
if ($Target -eq "aarch64-pc-windows-msvc" -and -not $hostIsArm64) {
    throw "Target aarch64-pc-windows-msvc needs an ARM64 host; an x64 host cannot execute ARM64 test binaries."
}

$targetArguments = if ($Target -eq "host") { @() } else { @("--target", $Target) }

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$resourceScript = Join-Path $PSScriptRoot "prepare-validation-resources.ps1"
$contractTest = Join-Path $PSScriptRoot "tests\verify.Tests.ps1"
$engineSupportTest = Join-Path $PSScriptRoot "tests\engine-support.Tests.ps1"
$runnerPrerequisiteTest = Join-Path $PSScriptRoot "tests\runner-prerequisites.Tests.ps1"
$devEnvironmentTest = Join-Path $PSScriptRoot "tests\dev-environment.Tests.ps1"
$rustDirectory = Join-Path $repositoryRoot "src-tauri"

# rustc is the host-architecture process no matter which target it emits, so the
# ARM64 compiler-stack limit applies to cross-builds too: parallel rustc against a
# *cold* target directory fails with STATUS_STACK_BUFFER_OVERRUN (0xc0000409) on
# several unrelated dependencies at once, which reads as a dependency problem and
# is not one.
#
# Scoped to a cold directory rather than applied to every ARM64 run, because
# serializing rustc unconditionally would slow every warm rebuild - locally and
# on a runner whose workspace persists - to fix a failure that only happens once.
# An explicit CARGO_BUILD_JOBS from the caller always wins.
$cargoTargetDirectory = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $rustDirectory "target"
} else {
    $env:CARGO_TARGET_DIR
}
$builtArtifactDirectory = if ($Target -eq "host") {
    $cargoTargetDirectory
} else {
    Join-Path $cargoTargetDirectory $Target
}
$targetDirectoryIsCold = -not (Test-Path -LiteralPath $builtArtifactDirectory)

if ($hostIsArm64 -and $targetDirectoryIsCold -and
    [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_JOBS)) {
    Write-Host "Cold target directory on ARM64: serializing rustc (CARGO_BUILD_JOBS=1)." -ForegroundColor Yellow
    $env:CARGO_BUILD_JOBS = "1"
}

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

function Invoke-NpmStep {
    param(
        [string]$Name,
        [string[]]$Arguments
    )

    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & npm @Arguments
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
    Write-Host "==> Engine support contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $engineSupportTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "==> Runner prerequisite contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $runnerPrerequisiteTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "==> Developer environment contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $devEnvironmentTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

& $resourceScript

Push-Location $rustDirectory
try {
    if ($Stage -in @("All", "Lint")) {
        # Formatting is architecture-independent, so it never takes a target.
        Invoke-CargoStep "Rust format" @("fmt", "--check")
        Invoke-CargoStep "Rust clippy" (@("clippy", "--locked") + $targetArguments + @("--", "-D", "warnings"))
    }

    if ($Stage -in @("All", "Test")) {
        Invoke-CargoStep "Rust unit tests" (@("test", "--locked") + $targetArguments + @("--lib"))
        Invoke-CargoStep "Rust IPC integration tests" (
            @("test", "--locked") + $targetArguments + @("--test", "integration_ipc")
        )
    }
} finally {
    Pop-Location
}

if ($Stage -in @("All", "Frontend")) {
    Push-Location $repositoryRoot
    try {
        Invoke-NpmStep "Install locked frontend dependencies" @("ci", "--ignore-scripts")
        Invoke-NpmStep "Product version synchronization" @("run", "version:check")
        # Cheap and early: a workflow that reaches for a paid hosted runner
        # should fail before a vite build, not after one.
        Invoke-NpmStep "Workflow runner policy" @("run", "runners:check")
        Invoke-NpmStep "Frontend format" @("run", "format:check")
        Invoke-NpmStep "Frontend lint" @("run", "lint")
        Invoke-NpmStep "Frontend typecheck" @("run", "typecheck")
        Invoke-NpmStep "Frontend production build" @("run", "build:web")
        Invoke-NpmStep "Maintainability ratchets" @("run", "maintainability")
        Invoke-NpmStep "Frontend unit tests" @("run", "test:frontend")
        Invoke-NpmStep "Browser bridge smoke" @("run", "test:browser")
        Invoke-NpmStep "Frontend dependency audit" @("audit", "--audit-level=high")
    } finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "Verification stage '$Stage' passed." -ForegroundColor Green
