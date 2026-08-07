<#
.SYNOPSIS
    Check, register, inspect, or remove a Meowcal Sub self-hosted Actions runner.

.DESCRIPTION
    A self-hosted runner executes repository code directly on this machine, so it
    is privileged infrastructure rather than a build convenience. Only the
    repository owner, or a maintainer the owner has explicitly approved, should
    run this in Install mode. Read docs/SELF_HOSTED_RUNNERS.md first.

    Check mode installs nothing and changes nothing. It reports every missing
    prerequisite in one pass rather than stopping at the first.

    Registration is a one-time act per host. This repository operates its runner
    on demand in the foreground and does NOT install it as a Windows service;
    see the operating model in docs/SELF_HOSTED_RUNNERS.md before using
    -Mode Install or -InstallService for anything other than a new or replaced
    host.

.EXAMPLE
    .\scripts\setup-self-hosted-runner.ps1 -Mode Status

.EXAMPLE
    .\scripts\setup-self-hosted-runner.ps1 -Mode Check

.EXAMPLE
    .\scripts\setup-self-hosted-runner.ps1 -Mode Install -Role all
#>
[CmdletBinding()]
param(
    [ValidateSet("Check", "Install", "Status", "Remove")]
    [string]$Mode = "Check",

    # `ci` needs to execute both architectures and therefore an ARM64 host.
    # `package` claims only what the host can prove it builds.
    [ValidateSet("ci", "package", "all")]
    [string]$Role = "all",

    [string]$Repository = "PeterShanxin/Meowcal-Sub",

    # Deliberately outside any repository checkout: the runner's work directory
    # must never sit inside a tree that `git clean -ffdx` can reach.
    [string]$RunnerDirectory = "C:\actions-runner\meowcal-sub",

    [string]$RunnerName = "$env:COMPUTERNAME-meowcal",

    [string]$RunnerVersion = "",

    # Not this repository's operating model: the runner is run on demand in the
    # foreground. Use only when the owner explicitly asks for a service.
    [switch]$InstallService,

    # Windows account the service runs as, for example 'DOMAIN\user'. Required in
    # practice whenever the Rust toolchain is user-scoped, because the default
    # NT AUTHORITY\NETWORK SERVICE account cannot see it. The runner's own
    # config.cmd prompts for the password; this script never handles one.
    [string]$ServiceAccount
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "runner-prerequisites.ps1")

function Write-Section {
    param([string]$Title)
    Write-Host ""
    Write-Host "==> $Title" -ForegroundColor Cyan
}

function Invoke-PrerequisiteChecks {
    param(
        [Parameter(Mandatory)][string]$HostArchitecture,
        [Parameter(Mandatory)][string[]]$Labels
    )

    $targets = Get-RequiredTargets -Labels $Labels
    $results = @()

    $results += Test-WindowsHost
    $results += if ($HostArchitecture -eq "unsupported") {
        New-PrerequisiteResult -Name "Host architecture" -Status Missing `
            -Detail ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) `
            -Fix "Only Windows ARM64 and x64 hosts are supported."
    } else {
        New-PrerequisiteResult -Name "Host architecture" -Status Ok -Detail $HostArchitecture
    }

    $results += if ($PSVersionTable.PSVersion.Major -ge 7) {
        New-PrerequisiteResult -Name "PowerShell 7" -Status Ok -Detail $PSVersionTable.PSVersion.ToString()
    } else {
        New-PrerequisiteResult -Name "PowerShell 7" -Status Missing `
            -Detail $PSVersionTable.PSVersion.ToString() -Fix "Install PowerShell 7; workflows use 'shell: pwsh'."
    }

    $git = Get-CommandVersion -Command "git"
    $results += if ($git) {
        New-PrerequisiteResult -Name "Git" -Status Ok -Detail $git
    } else {
        New-PrerequisiteResult -Name "Git" -Status Missing -Fix "Install Git for Windows."
    }

    $nodeMajor = Get-DeclaredEngineMajor -RepositoryRoot $repositoryRoot -Engine "node"
    $npmMajor = Get-DeclaredEngineMajor -RepositoryRoot $repositoryRoot -Engine "npm"
    if ($nodeMajor) {
        $results += Test-MajorVersion -Name "Node.js" -VersionText (Get-CommandVersion -Command "node" -Arguments @("-v")) `
            -ExpectedMajor $nodeMajor -Fix "Install Node.js $nodeMajor as declared in package.json engines."
    }
    if ($npmMajor) {
        $results += Test-MajorVersion -Name "npm" -VersionText (Get-CommandVersion -Command "npm" -Arguments @("-v")) `
            -ExpectedMajor $npmMajor -Fix "Install npm $npmMajor as declared in package.json engines."
    }

    $results += Test-RustToolchain -RequiredTargets $targets
    $results += Test-MsvcLinkers -HostArchitecture $HostArchitecture -RequiredTargets $targets
    $results += Test-WindowsSdk

    # OverlayHost is only published during packaging; CI uses the ignored
    # placeholder resources instead.
    if ($Labels -contains "meowcal-package-x64" -or $Labels -contains "meowcal-package-arm64") {
        $results += Test-DotnetSdk
    }
    if ($Labels -contains "meowcal-ci") {
        $results += Test-BrowserChannel
    }

    $results += Test-DiskSpace -Path $RunnerDirectory
    $results += Test-ServiceAccountToolchain
    $results += Test-LongPathSupport

    $results
}

function Write-PrerequisiteReport {
    param([Parameter(Mandatory)][object[]]$Results)

    foreach ($result in $Results) {
        $marker, $color = switch ($result.Status) {
            "Ok" { "  OK      ", "Green" }
            "Advisory" { "  ADVISORY", "Yellow" }
            default { "  MISSING ", "Red" }
        }
        $line = "$marker $($result.Name)"
        if ($result.Detail) { $line += ": $($result.Detail)" }
        Write-Host $line -ForegroundColor $color
        if ($result.Status -ne "Ok" -and $result.Fix) {
            Write-Host "            $($result.Fix)" -ForegroundColor DarkGray
        }
    }

    @($Results | Where-Object { $_.Status -eq "Missing" }).Count
}

function Get-RegistrationToken {
    param([Parameter(Mandatory)][string]$Repository)

    # Minted here through an authenticated gh session and held only in memory.
    # Registration tokens expire in one hour; none is ever written to disk, to a
    # log, or to the transcript.
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI not found. Install gh and run 'gh auth login', or export RUNNER_REGISTRATION_TOKEN yourself."
    }

    $token = gh api -X POST "repos/$Repository/actions/runners/registration-token" --jq .token 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($token)) {
        throw "Could not mint a registration token for $Repository. Check 'gh auth status' and repository admin rights."
    }
    $token.Trim()
}

function Get-RemoveToken {
    param([Parameter(Mandatory)][string]$Repository)

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI not found. Install gh and run 'gh auth login'."
    }
    $token = gh api -X POST "repos/$Repository/actions/runners/remove-token" --jq .token 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($token)) {
        throw "Could not mint a removal token for $Repository."
    }
    $token.Trim()
}

function Install-RunnerPackage {
    param(
        [Parameter(Mandatory)][string]$HostArchitecture,
        [Parameter(Mandatory)][string]$Directory,
        [string]$Version
    )

    if (Test-Path -LiteralPath (Join-Path $Directory "config.cmd")) {
        Write-Host "Runner package already present in $Directory."
        return
    }

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI not found. Install gh and run 'gh auth login'."
    }

    $releaseSelector = if ($Version) { "tags/v$Version" } else { "latest" }
    $release = gh api "repos/actions/runner/releases/$releaseSelector" --jq '{tag: .tag_name, body: .body}' |
        ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or -not $release) {
        throw "Could not read the actions/runner release '$releaseSelector'."
    }
    $Version = $release.tag.TrimStart("v")

    $runtimeId = if ($HostArchitecture -eq "arm64") { "win-arm64" } else { "win-x64" }
    $archive = "actions-runner-$runtimeId-$Version.zip"
    $url = "https://github.com/actions/runner/releases/download/v$Version/$archive"

    # There is no .sha256 sidecar asset. The runner project publishes each
    # package's digest inside the release notes, delimited by HTML comments, and
    # that is the only published hash for these archives.
    $digestPattern = "<!-- BEGIN SHA $runtimeId -->([0-9a-fA-F]{64})<!-- END SHA $runtimeId -->"
    $digestMatch = [regex]::Match($release.body, $digestPattern)
    if (-not $digestMatch.Success) {
        throw "No published SHA-256 for $archive in the release notes; refusing to install an unverified runner."
    }
    $expectedHash = $digestMatch.Groups[1].Value.ToUpperInvariant()

    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $downloadPath = Join-Path $Directory $archive

    Write-Host "Downloading $archive"
    Invoke-WebRequest -Uri $url -OutFile $downloadPath

    # The published hash is the only thing standing between this host and a
    # substituted runner binary, so a mismatch deletes the download rather than
    # leaving it somewhere convenient to run by hand.
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $downloadPath).Hash
    if ($actualHash -ne $expectedHash) {
        Remove-Item -LiteralPath $downloadPath -Force -ErrorAction SilentlyContinue
        throw "Runner archive hash mismatch. Expected $expectedHash, got $actualHash."
    }
    Write-Host "Runner archive SHA-256 verified: $actualHash"

    Expand-Archive -LiteralPath $downloadPath -DestinationPath $Directory -Force
    Remove-Item -LiteralPath $downloadPath -Force
}

function Register-Runner {
    param(
        [Parameter(Mandatory)][string]$Directory,
        [Parameter(Mandatory)][string]$Repository,
        [Parameter(Mandatory)][string[]]$Labels,
        [Parameter(Mandatory)][string]$Name,
        [switch]$AsService,
        [string]$ServiceAccount
    )

    $token = if ($env:RUNNER_REGISTRATION_TOKEN) {
        Write-Host "Using RUNNER_REGISTRATION_TOKEN from the environment."
        $env:RUNNER_REGISTRATION_TOKEN
    } else {
        Get-RegistrationToken -Repository $Repository
    }

    $arguments = @(
        "--unattended",
        "--replace",
        "--url", "https://github.com/$Repository",
        "--token", $token,
        "--name", $Name,
        "--labels", ($Labels -join ","),
        "--work", "_work"
    )
    if ($AsService) {
        $arguments += "--runasservice"
        if ($ServiceAccount) {
            # config.cmd prompts for this account's password itself. Never accept
            # a password through this script, and never put one in a command line
            # where it would land in history and in the transcript.
            $arguments += @("--windowslogonaccount", $ServiceAccount)
        } else {
            Write-Warning "No -ServiceAccount given: the service will run as NT AUTHORITY\NETWORK SERVICE, which cannot see a user-scoped Rust toolchain."
        }
    }

    Write-Host "Registering '$Name' with labels: $($Labels -join ', ')"
    Push-Location $Directory
    try {
        # The token is passed as an argument and never echoed. Do not add -Verbose
        # output around this call.
        & .\config.cmd @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "Runner registration failed with exit code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
        $token = $null
        Remove-Variable -Name token -ErrorAction SilentlyContinue
    }
}

function Show-RunnerStatus {
    param([Parameter(Mandatory)][string]$Repository)

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI not found; cannot read runner status."
    }

    $runners = gh api "repos/$Repository/actions/runners" --jq '.runners[] | "\(.name)\t\(.status)\t\(.busy)\t\([.labels[].name] | join(","))"' 2>$null
    if (-not $runners) {
        Write-Warning "No runners registered on $Repository. Self-hosted jobs will queue until one comes online."
        return
    }

    Write-Host "name`tstatus`tbusy`tlabels"
    $runners | ForEach-Object { Write-Host $_ }
}

$hostArchitecture = Get-HostArchitecture
$labelWarnings = @()
$labels = Resolve-RunnerLabels -HostArchitecture $hostArchitecture -Role $Role -Warnings ([ref]$labelWarnings)
foreach ($warning in $labelWarnings) { Write-Warning $warning }

Write-Section "Meowcal Sub self-hosted runner - $Mode"
Write-Host "Repository:   $Repository"
Write-Host "Host:         Windows $hostArchitecture"
Write-Host "Runner dir:   $RunnerDirectory"
Write-Host "Labels:       $($labels -join ', ')"

switch ($Mode) {
    "Check" {
        Write-Section "Prerequisites"
        $missing = Write-PrerequisiteReport -Results (Invoke-PrerequisiteChecks -HostArchitecture $hostArchitecture -Labels $labels)
        Write-Host ""
        if ($missing -gt 0) {
            Write-Host "$missing prerequisite(s) missing. Nothing was installed or changed." -ForegroundColor Red
            exit 1
        }
        Write-Host "All prerequisites satisfied. Re-run with -Mode Install to register." -ForegroundColor Green
    }

    "Install" {
        Write-Section "Prerequisites"
        $missing = Write-PrerequisiteReport -Results (Invoke-PrerequisiteChecks -HostArchitecture $hostArchitecture -Labels $labels)
        if ($missing -gt 0) {
            throw "$missing prerequisite(s) missing. Fix them and re-run; nothing was changed."
        }

        Write-Section "Runner package"
        Install-RunnerPackage -HostArchitecture $hostArchitecture -Directory $RunnerDirectory -Version $RunnerVersion

        Write-Section "Registration"
        Register-Runner -Directory $RunnerDirectory -Repository $Repository -Labels $labels `
            -Name $RunnerName -AsService:$InstallService -ServiceAccount $ServiceAccount

        Write-Section "Status"
        Show-RunnerStatus -Repository $Repository
        Write-Host ""
        if ($InstallService) {
            Write-Host "Registered as a service. It starts with Windows." -ForegroundColor Green
        } else {
            Write-Host "Registered. Start it with: $RunnerDirectory\run.cmd" -ForegroundColor Green
        }
    }

    "Status" {
        Write-Section "Registered runners"
        Show-RunnerStatus -Repository $Repository
    }

    "Remove" {
        if (-not (Test-Path -LiteralPath (Join-Path $RunnerDirectory "config.cmd"))) {
            throw "No runner installation found in $RunnerDirectory."
        }

        Write-Section "Removing runner registration"
        $token = Get-RemoveToken -Repository $Repository
        Push-Location $RunnerDirectory
        try {
            & .\config.cmd remove --token $token
            if ($LASTEXITCODE -ne 0) {
                throw "Runner removal failed with exit code $LASTEXITCODE."
            }
        } finally {
            Pop-Location
            $token = $null
            Remove-Variable -Name token -ErrorAction SilentlyContinue
        }
        Write-Host "Runner removed. The directory $RunnerDirectory is left in place for you to delete." -ForegroundColor Green
    }
}
