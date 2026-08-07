[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$scriptsDirectory = Split-Path -Parent $PSScriptRoot
. (Join-Path $scriptsDirectory "runner-prerequisites.ps1")

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-Lines {
    param([string[]]$Expected, [string[]]$Actual, [string]$Message)

    Assert-Equal $Expected.Count $Actual.Count "$Message count."
    for ($index = 0; $index -lt $Expected.Count; $index++) {
        Assert-Equal $Expected[$index] $Actual[$index] "$Message entry $index."
    }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$ExpectedSubstring, [string]$Message)

    try {
        & $Action
    } catch {
        if ($_.Exception.Message -notlike "*$ExpectedSubstring*") {
            throw "$Message Expected a message containing '$ExpectedSubstring', got '$($_.Exception.Message)'."
        }
        return
    }
    throw "$Message Expected a terminating error, but none was thrown."
}

# An ARM64 host is the only one that can execute both shipped architectures, so
# it is the only one allowed to claim every role.
$armAll = Resolve-RunnerLabels -HostArchitecture "arm64" -Role "all"
Assert-Lines @("meowcal-ci", "meowcal-package-x64", "meowcal-package-arm64") $armAll "ARM64 all-role labels"

$armCi = Resolve-RunnerLabels -HostArchitecture "arm64" -Role "ci"
Assert-Lines @("meowcal-ci") $armCi "ARM64 ci-role labels"

# An x64 host may package x64 and nothing else. Claiming meowcal-ci there would
# silently drop the ARM64 half of CI while still reporting success, and ARM64
# packaging has never been proven from an x64 host.
$warnings = @()
$x64All = Resolve-RunnerLabels -HostArchitecture "x64" -Role "all" -Warnings ([ref]$warnings)
Assert-Lines @("meowcal-package-x64") $x64All "x64 all-role labels"
Assert-Equal 2 $warnings.Count "x64 all-role warning count."

Assert-Throws { Resolve-RunnerLabels -HostArchitecture "x64" -Role "ci" } `
    "needs an ARM64 host" "x64 host refusing the ci role."

# The required Rust targets follow from the labels, so a host is never asked to
# install a toolchain for work it will not be given.
Assert-Lines @("aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc") `
    (Get-RequiredTargets -Labels $armAll) "ARM64 all-role targets"
Assert-Lines @("x86_64-pc-windows-msvc") `
    (Get-RequiredTargets -Labels $x64All) "x64 package-only targets"
Assert-Lines @("aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc") `
    (Get-RequiredTargets -Labels @("meowcal-ci")) "ci-only targets"

# A failed probe writes to stderr and still returns text. Treating that text as a
# version string reports a missing component as installed.
$missing = Get-CommandVersion -Command "cargo" -Arguments @("definitely-not-a-subcommand")
Assert-Equal $null $missing "A failing command probe must return null."

# The declared engine majors are read from package.json. Under
# Set-StrictMode -Version Latest, dot-notation access to an absent key throws and
# would abort the whole prerequisite report at the first gap, defeating the
# report-everything-in-one-pass contract. A missing key must return null instead.
$engineFixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "meowcal-engine-fixture-" + [guid]::NewGuid().ToString("N")
)
New-Item -ItemType Directory -Path $engineFixtureRoot | Out-Null
try {
    '{ "engines": { "node": ">=24 <25" } }' |
        Set-Content -LiteralPath (Join-Path $engineFixtureRoot "package.json") -Encoding utf8

    Assert-Equal 24 (Get-DeclaredEngineMajor -RepositoryRoot $engineFixtureRoot -Engine "node") `
        "Declared node major."
    Assert-Equal $null (Get-DeclaredEngineMajor -RepositoryRoot $engineFixtureRoot -Engine "npm") `
        "A package.json without an npm engine must return null, not throw."

    '{ "name": "no-engines" }' |
        Set-Content -LiteralPath (Join-Path $engineFixtureRoot "package.json") -Encoding utf8
    Assert-Equal $null (Get-DeclaredEngineMajor -RepositoryRoot $engineFixtureRoot -Engine "node") `
        "A package.json without an engines block must return null, not throw."

    Assert-Equal $null (Get-DeclaredEngineMajor -RepositoryRoot (Join-Path $engineFixtureRoot "absent") -Engine "node") `
        "A missing package.json must return null, not throw."
} finally {
    Remove-Item -LiteralPath $engineFixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "runner-prerequisites contract tests passed."
