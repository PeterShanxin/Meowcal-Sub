# Prerequisite probes for a Meowcal Sub self-hosted runner host.
#
# Dot-sourced by scripts/setup-self-hosted-runner.ps1. Every probe returns a
# result object rather than throwing, so one run reports everything that is
# wrong instead of stopping at the first thing.

Set-StrictMode -Version Latest

$script:CiLabel = "meowcal-ci"
$script:PackageX64Label = "meowcal-package-x64"
$script:PackageArm64Label = "meowcal-package-arm64"

function Resolve-RunnerLabels {
    <#
        Which roles a host may claim is a property of the host, not a preference.
        Windows x64 emulation runs one way only, so an ARM64 host can build and
        execute both shipped architectures and an x64 host can only do its own.
        A host that silently claimed `meowcal-ci` without being able to execute
        ARM64 tests would halve CI coverage and report success.
    #>
    param(
        [Parameter(Mandatory)][string]$HostArchitecture,
        [Parameter(Mandatory)][ValidateSet("ci", "package", "all")][string]$Role,
        [ref]$Warnings
    )

    $labels = @()
    $notes = @()

    if ($Role -in @("ci", "all")) {
        if ($HostArchitecture -eq "arm64") {
            $labels += $script:CiLabel
        } elseif ($Role -eq "ci") {
            throw "Role 'ci' needs an ARM64 host. This host is $HostArchitecture and cannot execute ARM64 test binaries."
        } else {
            $notes += "Skipping '$script:CiLabel': it needs an ARM64 host and this host is $HostArchitecture."
        }
    }

    if ($Role -in @("package", "all")) {
        $labels += $script:PackageX64Label
        if ($HostArchitecture -eq "arm64") {
            $labels += $script:PackageArm64Label
        } else {
            $notes += "Skipping '$script:PackageArm64Label': ARM64 packaging is only proven on an ARM64 host."
        }
    }

    if (-not $labels) {
        throw "No labels resolved for role '$Role' on a $HostArchitecture host."
    }

    if ($Warnings) { $Warnings.Value = $notes }
    $labels
}

function Get-RequiredTargets {
    param([Parameter(Mandatory)][string[]]$Labels)

    $targets = @()
    if ($Labels -contains $script:CiLabel) {
        # CI covers both architectures on one host; that is the whole point of
        # requiring an ARM64 host for the role.
        $targets += @("aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc")
    }
    if ($Labels -contains $script:PackageX64Label) { $targets += "x86_64-pc-windows-msvc" }
    if ($Labels -contains $script:PackageArm64Label) { $targets += "aarch64-pc-windows-msvc" }
    @($targets | Select-Object -Unique)
}

function New-PrerequisiteResult {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet("Ok", "Missing", "Advisory")][string]$Status,
        [string]$Detail = "",
        [string]$Fix = ""
    )

    [pscustomobject]@{
        Name   = $Name
        Status = $Status
        Detail = $Detail
        Fix    = $Fix
    }
}

function Get-HostArchitecture {
    # Read from the runtime rather than PROCESSOR_ARCHITECTURE, which reports
    # AMD64 to an emulated x64 process running on an ARM64 machine.
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "Arm64" { "arm64" }
        "X64" { "x64" }
        default { "unsupported" }
    }
}

function Get-CommandVersion {
    param(
        [Parameter(Mandatory)][string]$Command,
        [string[]]$Arguments = @("--version")
    )

    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        return $null
    }

    try {
        $output = & $Command @Arguments 2>&1 | Out-String
        # An exit code is the only reliable signal here. A failed probe still
        # writes to stderr, and treating that text as a version string reports a
        # missing component as present.
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return $output.Trim()
    } catch {
        return $null
    }
}

function Test-MajorVersion {
    param(
        [Parameter(Mandatory)][string]$Name,
        [string]$VersionText,
        [Parameter(Mandatory)][int]$ExpectedMajor,
        [Parameter(Mandatory)][string]$Fix
    )

    if (-not $VersionText) {
        return New-PrerequisiteResult -Name $Name -Status Missing -Detail "not installed" -Fix $Fix
    }

    $match = [regex]::Match($VersionText, '(\d+)\.(\d+)\.(\d+)')
    if (-not $match.Success) {
        return New-PrerequisiteResult -Name $Name -Status Missing `
            -Detail "unreadable version: $VersionText" -Fix $Fix
    }

    $major = [int]$match.Groups[1].Value
    $version = $match.Value
    if ($major -ne $ExpectedMajor) {
        return New-PrerequisiteResult -Name $Name -Status Missing `
            -Detail "found $version, need major $ExpectedMajor" -Fix $Fix
    }

    New-PrerequisiteResult -Name $Name -Status Ok -Detail $version
}

function Get-DeclaredEngineMajor {
    param(
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$Engine
    )

    # The supported majors are declared once, in package.json. Reading them back
    # keeps this script from becoming a second, silently stale source of truth.
    $packageJson = Join-Path $RepositoryRoot "package.json"
    if (-not (Test-Path -LiteralPath $packageJson)) {
        return $null
    }

    $engines = (Get-Content -Raw -LiteralPath $packageJson | ConvertFrom-Json).engines
    if (-not $engines -or -not $engines.$Engine) {
        return $null
    }

    $match = [regex]::Match($engines.$Engine, '>=\s*(\d+)')
    if ($match.Success) { [int]$match.Groups[1].Value } else { $null }
}

function Test-WindowsHost {
    if (-not $IsWindows) {
        return New-PrerequisiteResult -Name "Operating system" -Status Missing `
            -Detail "this host is not Windows" `
            -Fix "Meowcal Sub builds against Windows APIs. A runner host must run Windows 11."
    }

    $build = [System.Environment]::OSVersion.Version.Build
    if ($build -lt 22621) {
        return New-PrerequisiteResult -Name "Operating system" -Status Missing `
            -Detail "Windows build $build" `
            -Fix "OverlayHost targets net9.0-windows10.0.22621.0. Use Windows 11 22H2 or newer."
    }

    New-PrerequisiteResult -Name "Operating system" -Status Ok -Detail "Windows build $build"
}

function Test-RustToolchain {
    param([string[]]$RequiredTargets)

    $results = @()

    $rustc = Get-CommandVersion -Command "rustc" -Arguments @("--version")
    if (-not $rustc) {
        return @(New-PrerequisiteResult -Name "Rust toolchain" -Status Missing -Detail "rustc not on PATH" `
                -Fix "Install rustup from https://rustup.rs and reopen the shell.")
    }
    $results += New-PrerequisiteResult -Name "Rust toolchain" -Status Ok -Detail $rustc

    # The cargo subcommand does not always match the rustup component name:
    # rustfmt is installed as `rustfmt` but invoked as `cargo fmt`.
    $components = @(
        @{ Component = "rustfmt"; Subcommand = "fmt" },
        @{ Component = "clippy"; Subcommand = "clippy" }
    )
    foreach ($entry in $components) {
        $probe = Get-CommandVersion -Command "cargo" -Arguments @($entry.Subcommand, "--version")
        if ($probe) {
            $results += New-PrerequisiteResult -Name "Rust $($entry.Component)" -Status Ok `
                -Detail $probe.Split([Environment]::NewLine)[0]
        } else {
            $results += New-PrerequisiteResult -Name "Rust $($entry.Component)" -Status Missing -Detail "not installed" `
                -Fix "rustup component add $($entry.Component)"
        }
    }

    $installed = @()
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        $installed = @(rustup target list --installed 2>$null)
    }
    foreach ($target in $RequiredTargets) {
        if ($installed -contains $target) {
            $results += New-PrerequisiteResult -Name "Rust target $target" -Status Ok
        } else {
            $results += New-PrerequisiteResult -Name "Rust target $target" -Status Missing -Detail "not installed" `
                -Fix "rustup target add $target"
        }
    }

    $results
}

function Get-VisualStudioPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) {
        return $null
    }

    $found = & $vswhere -latest -products * -property installationPath 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $found) { return $null }
    ($found | Select-Object -First 1).Trim()
}

function Test-MsvcLinkers {
    param(
        [Parameter(Mandatory)][string]$HostArchitecture,
        [string[]]$RequiredTargets
    )

    $installPath = Get-VisualStudioPath
    if (-not $installPath) {
        return @(New-PrerequisiteResult -Name "Visual Studio C++ build tools" -Status Missing `
                -Detail "no Visual Studio installation found" `
                -Fix "Install Visual Studio Build Tools with 'Desktop development with C++' and the Windows SDK.")
    }

    $results = @(New-PrerequisiteResult -Name "Visual Studio C++ build tools" -Status Ok -Detail $installPath)
    $hostDirectory = if ($HostArchitecture -eq "arm64") { "Hostarm64" } else { "Hostx64" }
    $toolsRoot = Join-Path $installPath "VC\Tools\MSVC"

    foreach ($target in $RequiredTargets) {
        # Cross-linking is what lets one host cover both architectures, and a
        # missing cross linker fails deep inside a twenty-minute build with a
        # message about a dependency rather than about the toolchain.
        $targetDirectory = if ($target -like "aarch64*") { "arm64" } else { "x64" }
        $name = "MSVC linker $hostDirectory -> $targetDirectory"
        $linker = $null
        if (Test-Path -LiteralPath $toolsRoot) {
            $linker = Get-ChildItem -LiteralPath $toolsRoot -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending |
                ForEach-Object { Join-Path $_.FullName "bin\$hostDirectory\$targetDirectory\link.exe" } |
                Where-Object { Test-Path -LiteralPath $_ } |
                Select-Object -First 1
        }

        if ($linker) {
            $results += New-PrerequisiteResult -Name $name -Status Ok -Detail $linker
        } else {
            $results += New-PrerequisiteResult -Name $name -Status Missing -Detail "not found under $toolsRoot" `
                -Fix "In the Visual Studio Installer add the MSVC v143 build tools for the $targetDirectory target."
        }
    }

    $results
}

function Test-WindowsSdk {
    $registryPath = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0"
    $installFolder = (Get-ItemProperty -Path $registryPath -ErrorAction SilentlyContinue).InstallationFolder
    if ($installFolder -and (Test-Path -LiteralPath $installFolder)) {
        return New-PrerequisiteResult -Name "Windows SDK" -Status Ok -Detail $installFolder
    }

    New-PrerequisiteResult -Name "Windows SDK" -Status Missing -Detail "not registered" `
        -Fix "Install the Windows 11 SDK through the Visual Studio Installer."
}

function Test-DotnetSdk {
    $version = Get-CommandVersion -Command "dotnet" -Arguments @("--version")
    if (-not $version) {
        return New-PrerequisiteResult -Name ".NET SDK" -Status Missing -Detail "dotnet not on PATH" `
            -Fix "Install the .NET 9 SDK or newer; OverlayHost is published with dotnet publish."
    }

    $major = [int]([regex]::Match($version, '^(\d+)').Groups[1].Value)
    if ($major -lt 9) {
        return New-PrerequisiteResult -Name ".NET SDK" -Status Missing -Detail "found $version" `
            -Fix "OverlayHost targets net9.0. Install the .NET 9 SDK or newer."
    }

    New-PrerequisiteResult -Name ".NET SDK" -Status Ok -Detail $version
}

function Test-BrowserChannel {
    # The browser smoke uses the system Google Chrome channel, not a Playwright
    # download, so `npm ci --ignore-scripts` never provides it.
    $candidates = @(
        (Join-Path ${env:ProgramFiles} "Google\Chrome\Application\chrome.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe"),
        (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
    )
    $chrome = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
    if ($chrome) {
        return New-PrerequisiteResult -Name "Google Chrome (browser smoke)" -Status Ok -Detail $chrome
    }

    New-PrerequisiteResult -Name "Google Chrome (browser smoke)" -Status Missing -Detail "not installed" `
        -Fix "Install Google Chrome, or set MEOWCAL_BROWSER_CHANNEL to an installed channel such as msedge."
}

function Test-DiskSpace {
    param([Parameter(Mandatory)][string]$Path, [int]$RequiredGigabytes = 40)

    $qualifier = Split-Path -Qualifier ([System.IO.Path]::GetFullPath($Path))
    $drive = Get-PSDrive -Name $qualifier.TrimEnd(":") -ErrorAction SilentlyContinue
    if (-not $drive) {
        return New-PrerequisiteResult -Name "Disk space" -Status Advisory -Detail "could not read $qualifier"
    }

    $freeGb = [math]::Round($drive.Free / 1GB, 1)
    if ($freeGb -lt $RequiredGigabytes) {
        return New-PrerequisiteResult -Name "Disk space" -Status Missing `
            -Detail "$freeGb GB free on $qualifier" `
            -Fix "Two Rust target directories and the npm cache need roughly $RequiredGigabytes GB."
    }

    New-PrerequisiteResult -Name "Disk space" -Status Ok -Detail "$freeGb GB free on $qualifier"
}

function Test-ServiceAccountToolchain {
    <#
        rustup installs into the *user* profile and puts itself on the user PATH
        only. A runner service left on its default account (NT AUTHORITY\NETWORK
        SERVICE) therefore cannot see cargo at all, and its %USERPROFILE%\.cargo
        is a different, empty directory. Every Rust job then fails on a machine
        whose interactive shell builds the project perfectly, which is a
        genuinely confusing place to start debugging.
    #>
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (-not $cargo) {
        return New-PrerequisiteResult -Name "Toolchain visible to a service account" -Status Advisory `
            -Detail "cargo not resolved; cannot assess"
    }

    $machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine") -split ";" |
        Where-Object { $_ }
    $cargoDirectory = Split-Path -Parent $cargo
    $onMachinePath = $machinePath | Where-Object { $_.TrimEnd("\") -ieq $cargoDirectory.TrimEnd("\") }

    if ($onMachinePath) {
        return New-PrerequisiteResult -Name "Toolchain visible to a service account" -Status Ok `
            -Detail "cargo is on the machine PATH"
    }

    New-PrerequisiteResult -Name "Toolchain visible to a service account" -Status Advisory `
        -Detail "cargo is user-scoped at $cargoDirectory" `
        -Fix "Install the service under this user account (-ServiceAccount '$env:USERDOMAIN\$env:USERNAME'), or run the runner in the foreground. The default NETWORK SERVICE account cannot see this toolchain."
}

function Test-LongPathSupport {
    $enabled = (Get-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
            -Name LongPathsEnabled -ErrorAction SilentlyContinue).LongPathsEnabled
    if ($enabled -eq 1) {
        return New-PrerequisiteResult -Name "Long path support" -Status Ok
    }

    New-PrerequisiteResult -Name "Long path support" -Status Advisory -Detail "LongPathsEnabled is not 1" `
        -Fix "Deep cargo and npm paths can exceed MAX_PATH. Set HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled to 1."
}
