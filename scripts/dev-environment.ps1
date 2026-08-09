# Environment resolution for the developer launchers.
#
# `dev-tauri.cmd` and `dev-browser.cmd` are batch files. A batch file cannot
# discover a Visual Studio installation or choose a writable build root, so both
# used to hardcode one maintainer machine's paths and stopped working the moment
# that machine changed. This script is the single place those decisions are made,
# so the two launchers cannot drift apart and the decisions can be tested.
#
# Run it directly to see what the launchers will use:
#
#     pwsh -NoProfile -File scripts\dev-environment.ps1
#
# It prints `KEY=VALUE` lines that the launchers consume with `for /f`. Nothing
# else goes to stdout; diagnostics go to stderr so a launcher never sets a
# variable to an error message.

[CmdletBinding()]
param(
    # `CargoTargetDir` is what `dev-browser.cmd` asks for: browser dev mode
    # builds with the toolchain already on PATH and must not fail because a
    # Visual Studio installation could not be found.
    [ValidateSet("All", "CargoTargetDir", "VisualStudio")]
    [string]$Emit = "All"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-DeveloperHostArchitecture {
    <#
        The launcher has to tell VsDevCmd which host and target architecture to
        initialize, and hardcoding `arm64` breaks every x64 contributor. A 32-bit
        host process sees the emulated architecture in PROCESSOR_ARCHITECTURE and
        the real one in PROCESSOR_ARCHITEW6432, so the latter wins when set.
    #>
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }

    switch ($architecture) {
        "ARM64" { return "arm64" }
        "AMD64" { return "x64" }
        "x86" { return "x86" }
        default { throw "Unsupported host architecture '$architecture'." }
    }
}

function Get-VswherePath {
    <#
        vswhere ships with every Visual Studio 2017 and newer installer and lives
        at a fixed location under the 32-bit program files directory. That
        directory is read from the environment rather than assumed to be on C:.
    #>
    $roots = @(${env:ProgramFiles(x86)}, $env:ProgramFiles) | Where-Object { $_ }
    foreach ($root in $roots) {
        $candidate = Join-Path $root "Microsoft Visual Studio\Installer\vswhere.exe"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    return $null
}

function Get-VisualStudioDirectoryRank {
    <#
        Ranks a Visual Studio directory name so newest sorts first.

        The names mix two schemes: a release year ("2019", "2022") and a product
        version ("18"). Sorting them as text puts "2022" above "18" even though
        2022 is product version 17 and older, so a host with both would
        initialize the *older* toolchain while claiming newest-first. Years are
        mapped to their product version; anything else is read as a version
        already, and an unrecognisable name ranks lowest rather than throwing.
    #>
    param([Parameter(Mandatory)][string]$Name)

    $yearToVersion = @{ "2015" = 14; "2017" = 15; "2019" = 16; "2022" = 17 }
    if ($yearToVersion.ContainsKey($Name)) { return $yearToVersion[$Name] }

    $parsed = 0
    if ([int]::TryParse($Name, [ref]$parsed)) {
        # A future year-style directory this table does not know yet would
        # otherwise outrank every real product version by three orders of
        # magnitude.
        if ($parsed -ge 2000) { return $parsed - 2005 }
        return $parsed
    }
    return 0
}

function Select-NewestInstallation {
    <#
        vswhere does not promise an order, and neither does a directory listing.
        Without an explicit ordering the launcher could initialize a different
        toolchain from one run to the next on a host with several installations,
        which is the kind of difference that shows up as an unreproducible build
        failure. Newest-first is the same preference a developer would express by
        hand; an unparsable version sorts last rather than throwing.
    #>
    param([object[]]$Installations)

    @($Installations) |
        Where-Object { $_ } |
        Sort-Object -Descending -Property @{ Expression = {
            $version = $null
            if ($_.PSObject.Properties.Name -contains "installationVersion") {
                [void][System.Version]::TryParse($_.installationVersion, [ref]$version)
            }
            if ($version) { $version } else { [System.Version]::new(0, 0) }
        } }
}

function Get-VisualStudioInstallations {
    <#
        Every installation is considered, not just the newest, because the newest
        one may be a workload that has no C++ tools. Build Tools, Community,
        Professional and Enterprise are all products, and `-products *` is what
        stops Build Tools being silently skipped.
    #>
    $vswhere = Get-VswherePath
    if (-not $vswhere) { return @() }

    $raw = & $vswhere -all -prerelease -products * -format json 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $raw) { return @() }

    try {
        $parsed = ($raw | Out-String) | ConvertFrom-Json
    } catch {
        return @()
    }

    $usable = @($parsed) | Where-Object {
        $_ -and $_.PSObject.Properties.Name -contains "installationPath" -and $_.installationPath
    }

    @(Select-NewestInstallation -Installations $usable | ForEach-Object { $_.installationPath })
}

function Get-VisualStudioFallbackPaths {
    <#
        A host can carry a usable Visual Studio and no vswhere - an installer
        that was removed, or a layout copied onto the machine. Probing the two
        program files roots is a bounded last resort; it is not a filesystem
        scan.

        Directory names mix release years and product versions ("2022" is version
        17, "18" is version 18), so they are ranked rather than sorted as text -
        otherwise "2022" would sort above "18" and the launcher would pick the
        older toolchain while claiming newest-first.
    #>
    $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) | Where-Object { $_ }
    $found = @()
    foreach ($root in $roots) {
        $visualStudioRoot = Join-Path $root "Microsoft Visual Studio"
        if (-not (Test-Path -LiteralPath $visualStudioRoot -PathType Container)) { continue }

        # <root>\<year-or-version>\<edition>\Common7\Tools\VsDevCmd.bat
        $found += @(Get-ChildItem -LiteralPath $visualStudioRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object -Descending -Property @{ Expression = { Get-VisualStudioDirectoryRank -Name $_.Name } } |
            ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Directory -ErrorAction SilentlyContinue } |
            ForEach-Object { $_.FullName })
    }
    $found
}

function Test-WindowsSdkPresent {
    <#
        `CONTRIBUTING.md` lists the Windows SDK alongside the C++ tools, and a
        Visual Studio carrying MSVC without an SDK links nothing: the failure
        arrives deep inside a Rust build as a missing `windows.h` or an unresolved
        import, which is the late failure this discovery is meant to replace with
        an early message.

        Read from the registry rather than a guessed path, the same source
        `scripts/runner-prerequisites.ps1` uses, so an SDK installed off `C:` is
        still found.
    #>
    $registryPath = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0"
    $installFolder = (Get-ItemProperty -Path $registryPath -ErrorAction SilentlyContinue).InstallationFolder
    [bool]($installFolder -and (Test-Path -LiteralPath $installFolder))
}

function Test-VisualStudioCppInstallation {
    <#
        An installation only counts when it can actually initialize an MSVC
        environment. Reporting a Visual Studio that has no C++ tools sends the
        developer into a link failure twenty minutes into a build instead of a
        clear message here.
    #>
    param([Parameter(Mandatory)][string]$InstallationPath)

    $devCmd = Join-Path $InstallationPath "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path -LiteralPath $devCmd -PathType Leaf)) { return $false }

    Test-Path -LiteralPath (Join-Path $InstallationPath "VC\Tools\MSVC") -PathType Container
}

function Resolve-VisualStudioDevCmd {
    <#
        Returns the path to the VsDevCmd.bat that should initialize the build
        environment, or throws with the reason none is usable. MEOWCAL_VSDEVCMD
        overrides discovery entirely, for a host whose layout this cannot guess.
    #>
    param([string[]]$InstallationPaths, [string]$Override)

    if ($Override) {
        if (-not (Test-Path -LiteralPath $Override -PathType Leaf)) {
            throw "MEOWCAL_VSDEVCMD is set to '$Override', which is not a file."
        }
        return $Override
    }

    $usable = @($InstallationPaths | Where-Object { $_ -and (Test-VisualStudioCppInstallation -InstallationPath $_) })
    if ($usable.Count -eq 0) {
        throw @(
            "No Visual Studio installation with C++ build tools was found.",
            "Install 'Desktop development with C++' plus the Windows SDK through the Visual Studio Installer",
            "(Build Tools, Community, Professional and Enterprise all work), or set MEOWCAL_VSDEVCMD to the",
            "full path of a VsDevCmd.bat to use."
        ) -join " "
    }

    # The SDK is a separate component from the MSVC toolset, so a compiler can be
    # present without one. Checked after an installation is found, because "no
    # Visual Studio" and "Visual Studio without an SDK" need different fixes.
    if (-not (Test-WindowsSdkPresent)) {
        throw @(
            "Visual Studio was found at $($usable | Select-Object -First 1), but no Windows 10/11 SDK is registered.",
            "MSVC cannot link without it. Add the Windows SDK component through the Visual Studio Installer."
        ) -join " "
    }

    Join-Path ($usable | Select-Object -First 1) "Common7\Tools\VsDevCmd.bat"
}

function Resolve-CargoTargetDir {
    <#
        The launchers moved the Rust build output off the repository because a
        OneDrive-synced checkout locks files mid-build. The old value named one
        maintainer's D: volume, which stopped existing. LOCALAPPDATA is the
        smallest assumption that still satisfies the original reason: it is
        per-user, always writable, never synced by OneDrive, and carries no drive
        letter of its own.

        Naming any specific absolute path here - including this machine's current
        one - would recreate the defect for the next host. A developer who wants
        the output elsewhere, including one keeping an existing build cache, sets
        MEOWCAL_CARGO_TARGET_DIR once; an explicit CARGO_TARGET_DIR from the
        caller always wins over both.
    #>
    param([string]$CargoTargetDir, [string]$Override, [string]$LocalAppData)

    if ($CargoTargetDir) { return $CargoTargetDir }
    if ($Override) { return $Override }
    if (-not $LocalAppData) {
        throw "LOCALAPPDATA is not set, so no default build directory can be derived. Set CARGO_TARGET_DIR."
    }

    Join-Path $LocalAppData "meowcal-sub\cargo-build"
}

# Dot-sourcing (from the contract test) must not run the emit path.
if ($MyInvocation.InvocationName -eq ".") { return }

# Results are emitted under MEOWCAL_RESOLVED_* names rather than the variables
# they end up as. The launchers clear these before running this script, so a
# missing line is unambiguously a failure. Emitting CARGO_TARGET_DIR or
# MEOWCAL_VSDEVCMD directly would make that impossible: both are inputs a
# developer may already have exported, so an inherited value would survive a
# failed run and be read as a successful resolution.
$lines = @()

if ($Emit -in @("All", "CargoTargetDir")) {
    $targetDirectory = Resolve-CargoTargetDir `
        -CargoTargetDir $env:CARGO_TARGET_DIR `
        -Override $env:MEOWCAL_CARGO_TARGET_DIR `
        -LocalAppData $env:LOCALAPPDATA
    if (-not (Test-Path -LiteralPath $targetDirectory -PathType Container)) {
        New-Item -ItemType Directory -Path $targetDirectory -Force | Out-Null
    }
    $lines += "MEOWCAL_RESOLVED_CARGO_TARGET_DIR=$targetDirectory"
}

if ($Emit -in @("All", "VisualStudio")) {
    $installations = @(Get-VisualStudioInstallations)
    if ($installations.Count -eq 0) {
        $installations = @(Get-VisualStudioFallbackPaths)
    }

    $lines += "MEOWCAL_RESOLVED_VSDEVCMD=$(Resolve-VisualStudioDevCmd -InstallationPaths $installations -Override $env:MEOWCAL_VSDEVCMD)"
    $lines += "MEOWCAL_RESOLVED_HOST_ARCH=$(Get-DeveloperHostArchitecture)"
}

$lines | ForEach-Object { Write-Output $_ }
