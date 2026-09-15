[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$workflow = Get-Content -LiteralPath (Join-Path $repositoryRoot ".github/workflows/core-package.yml") -Raw

function Get-Step {
    param([string]$Name)
    $match = [regex]::Match($workflow, "(?ms)^      - name: $([regex]::Escape($Name))\r?`n(?<step>.*?)(?=^      - |\z)")
    if (-not $match.Success) { throw "Missing Core workflow step: $Name" }
    return $match.Groups["step"].Value
}

function Get-Run {
    param([string]$Step)
    $match = [regex]::Match($Step, '(?ms)^        run: \|\r?\n(?<code>.*)')
    if (-not $match.Success) { throw "Expected an inline PowerShell run block." }
    return [scriptblock]::Create([regex]::Replace($match.Groups["code"].Value, '(?m)^          ', ''))
}

$cache = Get-Step "Cache Core dependencies"
foreach ($required in @(
    'uses: Swatinem/rust-cache@[0-9a-f]{40}',
    'workspaces: core -> \.\./core-package-target',
    'shared-key: core-package-\$\{\{ steps\.package\.outputs\.target \}\}',
    "save-if: \$\{\{ github\.ref == 'refs/heads/main' \}\}"
)) {
    if ($cache -notmatch $required) { throw "Core cache contract missing: $required" }
}
if ($workflow -match 'continue-on-error: true|SkipExecutableContractCheck') {
    throw "Core packaging must fail closed and keep the executable handshake."
}
$testStep = Get-Step "Test Core release profile"
$packageStep = Get-Step "Build and package Core"
if ($testStep -match '(?m)^        if:' -or $packageStep -match '(?m)^        if:') {
    throw "Core test and package steps must use the default success-only condition."
}
if ($workflow.IndexOf("- name: Test Core release profile") -gt
    $workflow.IndexOf("- name: Build and package Core")) {
    throw "Core tests must gate packaging."
}
$testRun = Get-Run $testStep
$packageRun = Get-Run $packageStep
$parallelRun = Get-Run (Get-Step "Configure hosted build parallelism")
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("meowcal-core-workflow-" + [guid]::NewGuid().ToString("N"))
$environmentNames = @("GITHUB_WORKSPACE", "GITHUB_ENV", "CORE_ARCHITECTURE", "CORE_TARGET")
$previousExitCode = $global:LASTEXITCODE
$previousEnvironment = @{}
foreach ($name in $environmentNames) { $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }

function cargo {
    $script:recordedCargo = @($args)
    $global:LASTEXITCODE = $script:cargoExit
}

New-Item -ItemType Directory -Path (Join-Path $temporaryDirectory "scripts") -Force | Out-Null
Push-Location $temporaryDirectory
try {
    $env:GITHUB_WORKSPACE = $temporaryDirectory
    $env:GITHUB_ENV = Join-Path $temporaryDirectory "github-env"
    $expectedParallelism = "CARGO_BUILD_JOBS=$env:NUMBER_OF_PROCESSORS"
    & $parallelRun
    if ((Get-Content -LiteralPath $env:GITHUB_ENV -Raw).Trim() -ne $expectedParallelism) {
        throw "Hosted Core builds must expose the runner CPU count to subsequent steps."
    }
    @'
param($Architecture, $OutputDirectory, $CargoTargetDir)
@{ Architecture = $Architecture; OutputDirectory = $OutputDirectory; CargoTargetDir = $CargoTargetDir } |
    ConvertTo-Json | Set-Content -LiteralPath package-args.json
'@ | Set-Content -LiteralPath scripts/package-core.ps1
    foreach ($architecture in @("x64", "arm64")) {
        $env:CORE_ARCHITECTURE = $architecture
        $env:CORE_TARGET = if ($architecture -eq "arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
        $script:cargoExit = 0
        & $testRun
        $expected = @("test", "--release", "--manifest-path", "core/Cargo.toml", "--locked", "--target", $env:CORE_TARGET,
            "--target-dir", "$temporaryDirectory/core-package-target", "--all-targets")
        if (($script:recordedCargo -join "|") -ne ($expected -join "|")) {
            throw "Core packaging must test every target in the locked release profile: $script:recordedCargo"
        }
        & $packageRun
        $package = Get-Content -LiteralPath package-args.json -Raw | ConvertFrom-Json
        if ($package.Architecture -ne $architecture -or
            $package.CargoTargetDir -ne "$temporaryDirectory/core-package-target" -or
            $package.OutputDirectory -ne "$temporaryDirectory/core-release-assets") {
            throw "Tests, packaging, and the cache must use the same native target directory."
        }
        Remove-Item -LiteralPath package-args.json
        $script:cargoExit = 23
        $failed = $false
        try { & $testRun; & $packageRun } catch {
            if ($_.Exception.Message -notlike "Core tests failed for*") { throw }
            $failed = $true
        }
        if (-not $failed -or (Test-Path -LiteralPath package-args.json)) {
            throw "A failed Core test must prevent packaging."
        }
    }
    Write-Host "Core package workflow contract tests passed."
} finally {
    Pop-Location
    $global:LASTEXITCODE = $previousExitCode
    foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name]) }
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
}
