[CmdletBinding()]
param(
    [ValidateSet("All", "Lint", "Test", "Frontend")]
    [string]$Stage = "All",

    # "host" is the historical behavior: cargo picks the host triple and no
    # --target flag is passed. Naming a triple is what lets one machine cover
    # both shipped architectures, because the crate compiles genuinely different
    # code for each - see the cfg(target_arch) split in `engine_launch.rs`.
    [ValidateSet("host", "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc")]
    [string]$Target = "host",

    # A reviewed lock is the default consumer path when one exists. Keep the
    # source-built Core candidate available for development and Core changes,
    # but require callers to opt into it once a lock has been reviewed.
    [switch]$CoreSourceCandidate
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
$coreSourceCandidateArguments = if ($CoreSourceCandidate) {
    @("--features", "core-source-candidate")
} else {
    @()
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$resourceScript = Join-Path $PSScriptRoot "prepare-validation-resources.ps1"
$contractTest = Join-Path $PSScriptRoot "tests\verify.Tests.ps1"
$corePackageTest = Join-Path $PSScriptRoot "tests\core-package.Tests.ps1"
$coreUpgradeTest = Join-Path $PSScriptRoot "tests\core-upgrade.Tests.ps1"
$browserBackendTest = Join-Path $PSScriptRoot "tests\browser-backend.Tests.ps1"
$engineSupportTest = Join-Path $PSScriptRoot "tests\engine-support.Tests.ps1"
$devEnvironmentTest = Join-Path $PSScriptRoot "tests\dev-environment.Tests.ps1"
$rustDirectory = Join-Path $repositoryRoot "src-tauri"
$coreDirectory = Join-Path $repositoryRoot "core"
$coreTargetDirectory = Join-Path $coreDirectory "target"
$coreManifest = Get-Content -LiteralPath (Join-Path $coreDirectory "Cargo.toml") -Raw
$coreVersionMatch = [regex]::Match(
    $coreManifest,
    '(?ms)^\[package\].*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
)
if (-not $coreVersionMatch.Success) {
    throw "core/Cargo.toml must declare a major.minor.patch package version."
}
$coreVersion = $coreVersionMatch.Groups["version"].Value
$coreTarget = if ($Target -ne "host") {
    $Target
} elseif ($hostIsArm64) {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
}
$coreArchitecture = if ($coreTarget -eq "aarch64-pc-windows-msvc") { "arm64" } else { "x64" }

# rustc is the host-architecture process no matter which target it emits, so the
# ARM64 compiler-stack limit applies to cross-builds too: parallel rustc against a
# *cold* target directory fails with STATUS_STACK_BUFFER_OVERRUN (0xc0000409) on
# several unrelated dependencies at once, which reads as a dependency problem and
# is not one.
#
# Scoped to a cold directory rather than applied to every ARM64 run, because
# serializing rustc unconditionally would slow every warm local rebuild to fix a
# failure that only happens once. An explicit CARGO_BUILD_JOBS from the caller
# always wins.
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
    Write-Host "==> Core package contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $corePackageTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "==> Core upgrade automation contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $coreUpgradeTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "==> Browser backend startup contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $browserBackendTest
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    Write-Host "==> Engine support contract tests" -ForegroundColor Cyan
    & pwsh -NoProfile -File $engineSupportTest
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

if ($Stage -in @("All", "Lint", "Test")) {
    Push-Location $coreDirectory
    try {
        if ($Stage -in @("All", "Lint")) {
            Invoke-CargoStep "Core Rust format" @("fmt", "--manifest-path", "Cargo.toml", "--", "--check")
            Invoke-CargoStep "Core Rust clippy" @(
                "clippy", "--manifest-path", "Cargo.toml", "--locked",
                "--target", $coreTarget, "--target-dir", $coreTargetDirectory,
                "--all-targets", "--", "-D", "warnings"
            )
        }
        if ($Stage -in @("All", "Test")) {
            Invoke-CargoStep "Core Rust tests" @(
                "test", "--manifest-path", "Cargo.toml", "--locked",
                "--target", $coreTarget, "--target-dir", $coreTargetDirectory,
                "--all-targets"
            )
        }
        Invoke-CargoStep "Core release executable" @(
            "build", "--manifest-path", "Cargo.toml", "--locked",
            "--target", $coreTarget, "--target-dir", $coreTargetDirectory,
            "--bin", "meowcal-core", "--release"
        )
    } finally {
        Pop-Location
    }

    if ($env:MEOWCAL_VERIFY_CONTRACT_ACTIVE -ne "1") {
        $coreBinary = Join-Path $coreTargetDirectory "$coreTarget\release\meowcal-core.exe"
        & (Join-Path $PSScriptRoot "test-core-executable.ps1") `
            -BinaryPath $coreBinary `
            -ExpectedVersion $coreVersion | Out-Null
        & (Join-Path $PSScriptRoot "prepare-core-resource.ps1") `
            -Architecture $coreArchitecture `
            -Configuration Release `
            -BinaryPath $coreBinary | Out-Null
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }

        if ($Stage -in @("All", "Test")) {
            $coreLockPath = Join-Path $repositoryRoot "config\meowcal-core.lock.json"
            $coreExecutable = $coreBinary
            if ((Test-Path -LiteralPath $coreLockPath -PathType Leaf) -and -not $CoreSourceCandidate) {
                Write-Host "==> Fetch reviewed Core release" -ForegroundColor Cyan
                & (Join-Path $PSScriptRoot "fetch-meowcal-core.ps1") `
                    -Architecture $coreArchitecture `
                    -LockPath $coreLockPath | Out-Null
                if ($LASTEXITCODE -ne 0) {
                    exit $LASTEXITCODE
                }
                $coreExecutable = Join-Path $repositoryRoot "src-tauri\resources\core\meowcal-core.exe"
            } elseif ($CoreSourceCandidate) {
                Write-Host "Using explicit source-built Core candidate." -ForegroundColor Yellow
            } else {
                Write-Host "No reviewed Core lock exists; retaining the source-built candidate." -ForegroundColor Yellow
            }

            $previousCoreExecutable = $env:MEOWCAL_CORE_EXECUTABLE
            try {
                $env:MEOWCAL_CORE_EXECUTABLE = $coreExecutable
                Push-Location $rustDirectory
                try {
                    Invoke-CargoStep "Core consumer handshake" (
                        @("test", "--locked") + $targetArguments + $coreSourceCandidateArguments + @(
                            "--lib", "core_client::tests::real_core_handshake_status_and_shutdown",
                            "--", "--ignored", "--exact"
                        )
                    )
                } finally {
                    Pop-Location
                }
            } finally {
                $env:MEOWCAL_CORE_EXECUTABLE = $previousCoreExecutable
            }
        }
    }
}

Push-Location $rustDirectory
try {
    if ($Stage -in @("All", "Lint")) {
        # Formatting is architecture-independent, so it never takes a target.
        Invoke-CargoStep "Rust format" @("fmt", "--check")
        Invoke-CargoStep "Rust clippy" (@("clippy", "--locked") + $targetArguments + $coreSourceCandidateArguments + @("--", "-D", "warnings"))
    }

    if ($Stage -in @("All", "Test")) {
        Invoke-CargoStep "Rust unit tests" (@("test", "--locked") + $targetArguments + $coreSourceCandidateArguments + @("--lib"))
        Invoke-CargoStep "Rust IPC integration tests" (
            @("test", "--locked") + $targetArguments + $coreSourceCandidateArguments + @("--test", "integration_ipc")
        )
        # Outside src/ so they survive module moves unchanged: these pin the
        # command payload shapes the frontend reads, which a structural
        # refactor must not alter.
        Invoke-CargoStep "Rust command contract tests" (
            @("test", "--locked") + $targetArguments + $coreSourceCandidateArguments + @("--test", "command_contracts")
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
        # Also cheap and early. This is what keeps the ADR index, the
        # document-class list, and CONTRIBUTING's read order from quietly
        # pointing at a file somebody renamed.
        Invoke-NpmStep "Documentation links" @("run", "docs:check")
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
