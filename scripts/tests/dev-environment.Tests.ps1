[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$scriptsDirectory = Split-Path -Parent $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scriptsDirectory
. (Join-Path $scriptsDirectory "dev-environment.ps1")

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

# --- Build directory ---------------------------------------------------------

# Join-Path resolves the drive qualifier, so a fictional `R:\` root would throw
# inside the function under test rather than exercise it. These roots only have
# to exist as paths; nothing is written to them.
$fakeAppData = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-fake-appdata"
$fakeExplicit = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-fake-explicit"
$fakeOverride = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-fake-override"

# A caller who already chose a build directory keeps it. CI, packaging, and a
# developer with a faster volume all set CARGO_TARGET_DIR themselves, and a
# launcher that overrode it would silently rebuild into the wrong place.
Assert-Equal $fakeExplicit `
    (Resolve-CargoTargetDir -CargoTargetDir $fakeExplicit -Override $fakeOverride -LocalAppData $fakeAppData) `
    "An explicit CARGO_TARGET_DIR wins."

Assert-Equal $fakeOverride `
    (Resolve-CargoTargetDir -CargoTargetDir "" -Override $fakeOverride -LocalAppData $fakeAppData) `
    "MEOWCAL_CARGO_TARGET_DIR is used when CARGO_TARGET_DIR is unset."

# The default must carry no drive letter of its own. The previous hardcoded
# `D:\cargo-build` is exactly the failure this replaces: the volume stopped
# existing and the canonical launcher stopped working. Defaulting to this
# machine's replacement path would recreate the same defect one host later.
Assert-Equal (Join-Path $fakeAppData "meowcal-sub\cargo-build") `
    (Resolve-CargoTargetDir -CargoTargetDir "" -Override "" -LocalAppData $fakeAppData) `
    "The default build directory is derived from LOCALAPPDATA."

Assert-Throws { Resolve-CargoTargetDir -CargoTargetDir "" -Override "" -LocalAppData "" } `
    "Set CARGO_TARGET_DIR" "A host with no LOCALAPPDATA."

# --- Visual Studio discovery -------------------------------------------------

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-dev-environment-$([System.Guid]::NewGuid().ToString('N'))"

function New-VisualStudioFixture {
    param([Parameter(Mandatory)][string]$Name, [switch]$WithoutCpp, [switch]$WithoutDevCmd)

    $installation = Join-Path $fixtureRoot $Name
    if (-not $WithoutDevCmd) {
        $tools = Join-Path $installation "Common7\Tools"
        New-Item -ItemType Directory -Path $tools -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $tools "VsDevCmd.bat") -Value "@echo off" -Encoding ascii
    }
    if (-not $WithoutCpp) {
        New-Item -ItemType Directory -Path (Join-Path $installation "VC\Tools\MSVC\14.0.0") -Force | Out-Null
    }
    $installation
}

try {
    $complete = New-VisualStudioFixture -Name "Community"
    $noCpp = New-VisualStudioFixture -Name "NoCpp" -WithoutCpp
    $noDevCmd = New-VisualStudioFixture -Name "NoDevCmd" -WithoutDevCmd

    Assert-True (Test-VisualStudioCppInstallation -InstallationPath $complete) `
        "An installation with VsDevCmd.bat and MSVC tools is usable."
    Assert-True (-not (Test-VisualStudioCppInstallation -InstallationPath $noCpp)) `
        "An installation without C++ tools is not usable."
    Assert-True (-not (Test-VisualStudioCppInstallation -InstallationPath $noDevCmd)) `
        "An installation without VsDevCmd.bat is not usable."

    # An installation that cannot initialize MSVC must not be chosen just because
    # it was listed first. This is the case that used to send a developer into a
    # link failure deep inside a build instead of a message here.
    Assert-Equal (Join-Path $complete "Common7\Tools\VsDevCmd.bat") `
        (Resolve-VisualStudioDevCmd -InstallationPaths @($noCpp, $noDevCmd, $complete) -Override "") `
        "Discovery skips installations without C++ build tools."

    Assert-Throws { Resolve-VisualStudioDevCmd -InstallationPaths @($noCpp) -Override "" } `
        "No Visual Studio installation with C++ build tools was found" `
        "A host with no usable Visual Studio."

    Assert-Throws { Resolve-VisualStudioDevCmd -InstallationPaths @($complete) -Override (Join-Path $fixtureRoot "missing.bat") } `
        "which is not a file" "An override pointing at nothing."

    Assert-Equal (Join-Path $complete "Common7\Tools\VsDevCmd.bat") `
        (Resolve-VisualStudioDevCmd -InstallationPaths @() -Override (Join-Path $complete "Common7\Tools\VsDevCmd.bat")) `
        "An override is used even when discovery finds nothing."
} finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Deterministic ordering --------------------------------------------------

# A host with several installations must initialize the same toolchain on every
# run. vswhere promises no order, so an unordered result would build against one
# compiler today and another tomorrow, which reads as an unreproducible failure
# rather than as a configuration problem.
$ordered = Select-NewestInstallation -Installations @(
    [pscustomobject]@{ installationPath = "R:\vs\2019"; installationVersion = "16.11.0" },
    [pscustomobject]@{ installationPath = "R:\vs\18"; installationVersion = "18.0.1" },
    [pscustomobject]@{ installationPath = "R:\vs\2022"; installationVersion = "17.9.2" }
)
Assert-Equal "R:\vs\18" $ordered[0].installationPath "The newest installation is preferred."
Assert-Equal "R:\vs\2019" $ordered[2].installationPath "The oldest installation sorts last."

# An installation whose version vswhere did not report must not throw and must
# not win over one that reported a real version.
$withMissingVersion = Select-NewestInstallation -Installations @(
    [pscustomobject]@{ installationPath = "R:\vs\unknown" },
    [pscustomobject]@{ installationPath = "R:\vs\17"; installationVersion = "17.0.0" }
)
Assert-Equal "R:\vs\17" $withMissingVersion[0].installationPath `
    "An installation with no reported version does not outrank a real one."

# --- Host architecture -------------------------------------------------------

# Hardcoding arm64 is what made the launcher a single-machine script. The value
# has to come from the host, and the 64-bit answer wins in a 32-bit process.
$architecture = Get-DeveloperHostArchitecture
Assert-True ($architecture -in @("arm64", "x64", "x86")) `
    "Host architecture resolved to an unexpected value '$architecture'."

# --- The launchers do not reintroduce a hardcoded machine path ---------------

# This is the regression guard for the defect itself: a launcher that names an
# absolute drive path again is back to working on exactly one machine.
foreach ($launcher in @("dev-tauri.cmd", "dev-browser.cmd")) {
    $contents = Get-Content -LiteralPath (Join-Path $repositoryRoot $launcher) -Raw
    $absolutePaths = [regex]::Matches($contents, '(?m)[A-Za-z]:\\[^"\r\n%]*')
    Assert-Equal 0 $absolutePaths.Count `
        "$launcher names absolute path(s) $(($absolutePaths | ForEach-Object { $_.Value }) -join ', ')."
}

Write-Host "dev-environment contract tests passed." -ForegroundColor Green
