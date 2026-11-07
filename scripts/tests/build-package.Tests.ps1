$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('meowcal-package-tests-' + [guid]::NewGuid().ToString('N'))
$environmentNames = @('TAURI_CONFIG', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'CARGO_PROFILE_RELEASE_CODEGEN_UNITS',
    'CARGO_PROFILE_RELEASE_LTO', 'CARGO_PROFILE_RELEASE_STRIP', 'CARGO_INCREMENTAL', 'MEOWCAL_CORE_EXECUTABLE', 'PROCESSOR_ARCHITECTURE')
$savedEnvironment = @{}
foreach ($name in $environmentNames) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }
$previousExitCode = $global:LASTEXITCODE
$previousDirectory = [Environment]::CurrentDirectory
$previousTestState = Get-Variable meowcalPackageTestState -Scope Global -ErrorAction SilentlyContinue
$global:meowcalPackageTestState = @{ Commands = $null; Failure = '' }

function Assert-Equal($Expected, $Actual, [string]$Message) {
    if ($Expected -cne $Actual) { throw "$Message Expected '$Expected', got '$Actual'." }
}

function cargo {
    $global:meowcalPackageTestState.Commands.Add([pscustomobject]@{
        Arguments = @($args)
        Jobs = $env:CARGO_BUILD_JOBS
        Lto = $env:CARGO_PROFILE_RELEASE_LTO
        Core = $env:MEOWCAL_CORE_EXECUTABLE
    })
    $isHandshake = $args -contains 'core_client::tests::real_core_handshake_status_and_shutdown'
    $shouldFail = ($isHandshake -and $global:meowcalPackageTestState.Failure -eq 'handshake') -or
        (-not $isHandshake -and $global:meowcalPackageTestState.Failure -eq 'store-tests')
    $global:LASTEXITCODE = if ($shouldFail) { 23 } else { 0 }
    if ($global:meowcalPackageTestState.Failure -ne 'empty-handshake') { 'test result: ok. 1 passed; 0 failed;' }
}

function npx {
    $global:meowcalPackageTestState.Commands.Add([pscustomobject]@{
        Arguments = @($args)
        Jobs = $env:CARGO_BUILD_JOBS
        Lto = $env:CARGO_PROFILE_RELEASE_LTO
        Core = $env:MEOWCAL_CORE_EXECUTABLE
    })
    $global:LASTEXITCODE = if ($global:meowcalPackageTestState.Failure -eq 'tauri') { 29 } else { 0 }
    $target = $args[[array]::IndexOf($args, '--target') + 1]
    $bundle = Join-Path $env:CARGO_TARGET_DIR "$target/release/bundle/nsis"
    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $bundle 'fixture-setup.exe') -Value 'fixture'
    $profile = if ($args -contains '--debug') { 'debug' } else { 'release' }
    New-Item -ItemType Directory -Path (Join-Path $env:CARGO_TARGET_DIR "$target/$profile") -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$target/$profile/meowcal-sub.exe") -Value 'fixture'
}

function Write-FixtureMsix {
    $packagePath = $args[[array]::IndexOf($args, '/p') + 1]
    Set-Content -LiteralPath $packagePath -Value 'fixture msix'
    $global:LASTEXITCODE = if ($global:meowcalPackageTestState.Failure -eq 'makeappx') { 31 } else { 0 }
}

New-Item -ItemType Directory -Path "$fixture/scripts", "$fixture/process-cwd" -Force | Out-Null
Push-Location $fixture
[Environment]::CurrentDirectory = "$fixture/process-cwd"
try {
    @'
param($Architecture)
if ($global:meowcalPackageTestState.Failure -eq 'fetch') { throw 'Fixture fetch failed.' }
'@ | Set-Content "$fixture/scripts/fetch-meowcal-core.ps1"
    @'
param($Architecture)
$global:LASTEXITCODE = 0
'@ | Set-Content "$fixture/scripts/build-overlayhost.ps1"
    Copy-Item "$repositoryRoot/scripts/package-build.psm1" "$fixture/scripts/"
    @'
function Assert-StoreIdentity { }
function Get-StoreMakeAppx { 'Write-FixtureMsix' }
function Assert-StorePeArchitecture { }
function New-StoreManifest { '<Package />' }
Export-ModuleMember -Function *
'@ | Set-Content "$fixture/scripts/store-package.psm1"
    foreach ($path in @('src-tauri/resources/core', 'src-tauri/icons', 'licenses')) {
        New-Item -ItemType Directory -Path (Join-Path $fixture $path) -Force | Out-Null
    }
    foreach ($file in @('meowcal-core.exe', 'meowcal-core.json', 'LICENSE')) {
        Set-Content "$fixture/src-tauri/resources/core/$file" 'fixture'
    }
    foreach ($file in @('Square150x150Logo.png', 'Square44x44Logo.png', 'StoreLogo.png')) {
        Set-Content "$fixture/src-tauri/icons/$file" 'fixture'
    }
    Set-Content "$fixture/src-tauri/tauri.store.conf.json" '{}'
    Set-Content "$fixture/LICENSE" 'fixture'
    Set-Content "$fixture/LICENSE-NOTICE.md" 'fixture'
    foreach ($variant in @('package', 'store')) {
        $source = Get-Content "$repositoryRoot/scripts/build-$variant.ps1" -Raw
        $hostExpression = '\[(System\.)?Runtime\.InteropServices\.RuntimeInformation\]::OSArchitecture'
        Assert-Equal 1 ([regex]::Matches($source, $hostExpression).Count) 'Fixture host substitution must be unambiguous.'

        foreach ($case in @(
            @{ HostArchitecture = 'Arm64'; Architecture = 'arm64'; Jobs = '4'; ExpectedJobs = '4' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'arm64'; Jobs = ''; ExpectedJobs = '1' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'x64'; Jobs = ''; ExpectedJobs = '1' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'x64'; Jobs = '2'; ExpectedJobs = '2' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'arm64'; Jobs = '-1'; ExpectedJobs = '-1' },
            @{ HostArchitecture = 'X64'; Architecture = 'x64'; Jobs = '3'; ExpectedJobs = '3' },
            @{ HostArchitecture = 'X64'; Architecture = 'x64'; Jobs = ''; ExpectedJobs = '' },
            @{ HostArchitecture = 'X64'; Architecture = 'auto'; Jobs = '4'; ExpectedJobs = '4' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'arm64'; Jobs = '2'; ExpectedJobs = '2'; Configuration = 'Debug' },
            @{ HostArchitecture = 'Arm64'; Architecture = 'auto'; Jobs = ' '; ExpectedJobs = '1' }
        )) {
            # Run the real script with a substituted OS architecture on either CI host.
            [regex]::Replace($source, $hostExpression, "'$($case.HostArchitecture)'") |
                Set-Content "$fixture/scripts/build-$variant.ps1"
            $failureModes = @('', 'fetch', 'handshake', 'empty-handshake', 'tauri', 'report')
            if ($variant -eq 'store') { $failureModes += @('store-tests', 'makeappx') }
            foreach ($failureMode in $failureModes) {
                $global:meowcalPackageTestState.Failure = $failureMode
                foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, 'caller-value') }
                $env:CARGO_BUILD_JOBS = $case.Jobs
                $env:PROCESSOR_ARCHITECTURE = 'AMD64'
                $before = @{}
                foreach ($name in $environmentNames) { $before[$name] = [Environment]::GetEnvironmentVariable($name) }
                $global:meowcalPackageTestState.Commands = [Collections.Generic.List[object]]::new()
                $targetDirectory = Join-Path $fixture ([guid]::NewGuid().ToString('N'))
                $targetArgument = Split-Path $targetDirectory -Leaf
                if ($failureMode -eq 'report') {
                    New-Item -ItemType Directory -Path (Join-Path $targetDirectory 'package-build.json') -Force | Out-Null
                }
                $caught = $null
                try {
                    $arguments = @{ Architecture = $case.Architecture; CargoTargetDir = $targetArgument }
                    if ($variant -eq 'store') {
                        $arguments += @{ PackageName = 'Fixture'; Publisher = 'CN=Fixture'; PublisherDisplayName = 'Fixture';
                            PackageVersion = '1.0.0.0'; OutputDirectory = (Join-Path $targetArgument 'output') }
                        if ($case.Configuration) { $arguments['Configuration'] = $case.Configuration }
                    }
                    & "$fixture/scripts/build-$variant.ps1" @arguments 6>$null | Out-Null
                } catch { $caught = $_ }
                if ($global:meowcalPackageTestState.Failure) {
                    if (-not $caught) { throw "Packaging accepted $failureMode failure." }
                } elseif ($caught) { throw $caught }
                foreach ($command in $global:meowcalPackageTestState.Commands) {
                    Assert-Equal $case.ExpectedJobs $command.Jobs 'Caller parallelism or local ARM64 default was lost.'
                    $target = if ($case.Architecture -eq 'arm64' -or
                        ($case.Architecture -eq 'auto' -and $case.HostArchitecture -eq 'Arm64')) { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
                    if ($command.Arguments -notcontains $target) { throw 'Wrong package target.' }
                    $expectedLto = if ($target -like 'aarch64*') { 'false' } else { 'caller-value' }
                    Assert-Equal $expectedLto $command.Lto 'Target-specific compiler safeguards changed.'
                    if ($command.Arguments -contains 'core_client::tests::real_core_handshake_status_and_shutdown') {
                        Assert-Equal (Join-Path $fixture 'src-tauri/resources/core/meowcal-core.exe') $command.Core 'Handshake must execute the fetched Core.'
                    }
                }
                if ($global:meowcalPackageTestState.Failure -in @('handshake', 'empty-handshake', 'fetch', 'store-tests')) {
                    if (@($global:meowcalPackageTestState.Commands | Where-Object { $_.Arguments -contains 'tauri' }).Count) {
                        throw 'Packaging continued after a failed Core prerequisite.'
                    }
                }
                foreach ($name in $environmentNames) {
                    Assert-Equal $before[$name] ([Environment]::GetEnvironmentVariable($name)) "Packaging leaked $name into the caller."
                }
                Assert-Equal $fixture (Get-Location).Path 'Packaging changed the caller directory.'
                if ($failureMode -eq 'report') { continue }
                $report = Get-Content (Join-Path $targetDirectory 'package-build.json') -Raw | ConvertFrom-Json
                Assert-Equal (-not [bool]$failureMode) $report.succeeded 'Build report success must reflect the build result.'
                Assert-Equal $case.ExpectedJobs $report.cargoBuildJobs 'Build report must record the effective parallelism.'
                if ($failureMode) {
                    Assert-Equal $false $report.steps[-1].succeeded 'Failed stage must remain in the report.'
                    $expectedStage = switch ($failureMode) {
                        'fetch' { 'Fetch pinned Core' }
                        { $_ -in @('handshake', 'empty-handshake') } { 'Core consumer handshake' }
                        'store-tests' { 'Store application tests' }
                        'makeappx' { 'Validate and pack MSIX' }
                        'tauri' { if ($variant -eq 'store') { 'Build Store application' } else { 'Build and sign Tauri bundles' } }
                    }
                    Assert-Equal $expectedStage $report.steps[-1].name 'Failure must stop at the expected stage.'
                } elseif (@($report.steps | Where-Object { -not $_.succeeded }).Count) { throw 'Successful build reported a failed stage.' }
                if (-not $global:meowcalPackageTestState.Failure) {
                    $expectedCommands = if ($variant -eq 'store') { 3 } else { 2 }
                    Assert-Equal $expectedCommands $global:meowcalPackageTestState.Commands.Count 'Packaging must execute its tests and Tauri build.'
                    if ($global:meowcalPackageTestState.Commands[-1].Arguments -notcontains '--locked') { throw 'The product build must use its Cargo lockfile.' }
                }
            }
        }
        [regex]::Replace($source, $hostExpression, "'X64'") | Set-Content "$fixture/scripts/build-$variant.ps1"
        $global:meowcalPackageTestState.Commands.Clear()
        $rejected = $false
        try {
            $arguments = @{ Architecture = 'arm64'; CargoTargetDir = (Join-Path $fixture 'wrong-host') }
            if ($variant -eq 'store') {
                $arguments += @{ PackageName = 'Fixture'; Publisher = 'CN=Fixture'; PublisherDisplayName = 'Fixture'; PackageVersion = '1.0.0.0' }
            }
            & "$fixture/scripts/build-$variant.ps1" @arguments | Out-Null
        } catch { $rejected = $_.Exception.Message -like 'ARM64 packaging requires an ARM64 host*' }
        if (-not $rejected -or $global:meowcalPackageTestState.Commands.Count) { throw 'An x64 host must refuse ARM64 test execution before building.' }
    }
    Write-Host 'Package parallelism, target selection, failure and environment contracts passed.'
} finally {
    foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name]) }
    $global:LASTEXITCODE = $previousExitCode
    Pop-Location
    [Environment]::CurrentDirectory = $previousDirectory
    if ($previousTestState) { $global:meowcalPackageTestState = $previousTestState.Value }
    else { Remove-Variable meowcalPackageTestState -Scope Global }
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
& (Join-Path $PSScriptRoot 'package-workflow.Tests.ps1')
