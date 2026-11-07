$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$action = Get-Content "$repositoryRoot/.github/actions/windows-package-build/action.yml" -Raw
function Get-ActionRun([string]$Name) {
    $step = [regex]::Match($action, "(?ms)^    - name: $([regex]::Escape($Name))\r?`n(?<step>.*?)(?=^    - |\z)")
    $run = [regex]::Match($step.Groups['step'].Value, '(?ms)^      run: \|\r?\n(?<code>.*)')
    if (-not $run.Success) { throw "Missing action run block: $Name" }
    [regex]::Replace($run.Groups['code'].Value, '(?m)^        ', '')
}
foreach ($required in @(
    'Swatinem/rust-cache@[0-9a-f]{40}',
    'if: inputs.use-rust-cache == ''true''',
    'workspaces: src-tauri -> \.\./\$\{\{ inputs.target-directory \}\}',
    'shared-key: package-\$\{\{ inputs.variant \}\}-\$\{\{ inputs.target \}\}',
    'cache-workspace-crates: false',
    'save-if: \$\{\{ github.ref == ''refs/heads/main'' \}\}'
)) {
    if ($action -notmatch $required) { throw "Package dependency cache contract missing: $required" }
}
foreach ($variant in @('package', 'store-package')) {
    $workflow = Get-Content "$repositoryRoot/.github/workflows/$variant.yml" -Raw
    if ([regex]::Matches($workflow, '(?ms)^      use_rust_cache:\r?\n(?:(?!^      \w).)*?^        default: false\r?$').Count -ne 2) {
        throw 'Manual and reusable packaging must leave target caching opt-in.'
    }
    foreach ($required in @(
        'uses: ./.github/actions/windows-package-build',
        'arm64-build-jobs: ${{ inputs.arm64_build_jobs }}',
        'use-rust-cache: ${{ inputs.use_rust_cache }}',
        'if: always()'
    )) {
        if (-not $workflow.Contains($required)) { throw "$variant must use the shared build policy and preserve failure timings: $required" }
    }
    if ($workflow -match 'continue-on-error: true') { throw 'Package failures must not be ignored.' }
    $partition = if ($variant -eq 'package') { 'direct' } else { 'store' }
    $directory = if ($variant -eq 'package') { 'package-output' } else { 'store-target' }
    if (-not $workflow.Contains("variant: $partition") -or -not $workflow.Contains("target-directory: $directory")) {
        throw 'Direct and Store Cargo outputs must use separate cache partitions.'
    }
}
foreach ($name in @('release', 'release-preflight')) {
    $workflow = Get-Content "$repositoryRoot/.github/workflows/$name.yml" -Raw
    if (-not $workflow.Contains('pattern: meowcal-sub-*')) { throw 'Release assets must exclude build report artifacts.' }
    if ($workflow -notmatch '(?ms)^      use_rust_cache:\r?\n(?:(?!^      \w).)*?^        default: false\r?$' -or
        [regex]::Matches($workflow, 'use_rust_cache: \$\{\{ inputs.use_rust_cache \}\}').Count -ne 3) {
        throw 'Release and preflight must forward opt-in caching to every package variant.'
    }
}
$preflight = Get-Content "$repositoryRoot/.github/workflows/release-preflight.yml" -Raw
if ([regex]::Matches($preflight, 'use_rust_cache: \$\{\{ inputs.use_rust_cache \}\}').Count -ne 3 -or
    [regex]::Matches($preflight, 'arm64_build_jobs: \$\{\{ inputs.arm64_build_jobs \}\}').Count -ne 2) {
    throw 'Preflight must apply benchmark settings to the x64 control and both ARM64 package variants.'
}

$fixture = Join-Path ([IO.Path]::GetTempPath()) ('meowcal-package-workflow-' + [guid]::NewGuid().ToString('N'))
$environmentNames = @('ARM64_BUILD_JOBS', 'GITHUB_ENV', 'GITHUB_WORKSPACE', 'PACKAGE_TARGET_DIRECTORY', 'PACKAGE_TARGET')
$savedEnvironment = @{}
foreach ($name in $environmentNames) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }
New-Item -ItemType Directory -Path $fixture | Out-Null
try {
    $parallel = Get-ActionRun 'Configure hosted build parallelism'
    foreach ($hostArchitecture in @('Arm64', 'X64')) {
        $run = [scriptblock]::Create($parallel.Replace('[Runtime.InteropServices.RuntimeInformation]::OSArchitecture', "'$hostArchitecture'"))
        foreach ($jobs in @('1', '2', '4')) {
            $env:ARM64_BUILD_JOBS = $jobs
            $env:GITHUB_ENV = Join-Path $fixture ([guid]::NewGuid().ToString('N'))
            & $run
            $expected = if ($hostArchitecture -eq 'Arm64') { $jobs } else { [Environment]::ProcessorCount }
            if ((Get-Content $env:GITHUB_ENV -Raw).Trim() -ne "CARGO_BUILD_JOBS=$expected") { throw 'Wrong hosted package parallelism.' }
        }
        foreach ($invalid in @('', '0', '8', 'four')) {
            $env:ARM64_BUILD_JOBS = $invalid
            $rejected = $false
            try { & $run } catch { $rejected = $_.Exception.Message -eq 'Hosted ARM64 build jobs must be 1, 2 or 4.' }
            if (-not $rejected) { throw "Invalid parallelism accepted: $invalid" }
        }
    }
    $discard = [scriptblock]::Create((Get-ActionRun 'Discard cached installers'))
    $env:GITHUB_WORKSPACE = $fixture
    foreach ($directory in @('package-output', 'store-target')) {
        $env:PACKAGE_TARGET_DIRECTORY = $directory
        foreach ($target in @('aarch64-pc-windows-msvc', 'x86_64-pc-windows-msvc')) {
            $env:PACKAGE_TARGET = $target
            $release = Join-Path $fixture "$directory/$target/release"
            New-Item -ItemType Directory -Path "$release/bundle/nsis" -Force | Out-Null
            Set-Content "$release/bundle/nsis/stale-setup.exe" 'stale'
            Set-Content "$release/dependency.rlib" 'keep'
            & $discard
            & $discard
            if ((Test-Path "$release/bundle") -or -not (Test-Path "$release/dependency.rlib")) {
                throw 'Cache restore must discard installers and retain compiled dependencies.'
            }
        }
    }
    Write-Host 'Hosted package parallelism, cache isolation and benchmark contracts passed.'
} finally {
    foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name]) }
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
