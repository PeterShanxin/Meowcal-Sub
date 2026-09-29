[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageName,
    [Parameter(Mandatory)][string]$Publisher,
    [Parameter(Mandatory)][string]$PublisherDisplayName,
    [Parameter(Mandatory)][string]$PackageVersion,
    [ValidateSet('auto', 'x64', 'arm64')][string]$Architecture = 'auto',
    [ValidateSet('Release', 'Debug')][string]$Configuration = 'Release',
    [string]$CargoTargetDir,
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'store-package.psm1') -Force
Assert-StoreIdentity $PackageName $Publisher $PublisherDisplayName $PackageVersion
$repositoryRoot = Split-Path $PSScriptRoot -Parent
$hostArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Architecture -eq 'auto') { $Architecture = if ($hostArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' } }
if ($Architecture -eq 'arm64' -and $hostArchitecture -ne 'Arm64') { throw 'ARM64 packaging requires an ARM64 host.' }
$target = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$makeAppx = Get-StoreMakeAppx
if (-not $CargoTargetDir) { $CargoTargetDir = Join-Path ([IO.Path]::GetTempPath()) "meowcal-sub-store-$Architecture" }
$CargoTargetDir = [IO.Path]::GetFullPath($CargoTargetDir)
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $CargoTargetDir ("store-" + [guid]::NewGuid().ToString('N')) }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'OutputDirectory must not exist; refusing to mix stale package files.' }

$environmentNames = @('TAURI_CONFIG', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_PROFILE_RELEASE_CODEGEN_UNITS', 'CARGO_PROFILE_RELEASE_LTO', 'CARGO_PROFILE_RELEASE_STRIP', 'CARGO_INCREMENTAL')
$savedEnvironment = @{}
foreach ($name in $environmentNames) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $repositoryRoot
try {
    $env:CARGO_TARGET_DIR = $CargoTargetDir
    $env:TAURI_CONFIG = Get-Content src-tauri/tauri.store.conf.json -Raw
    if ($hostArchitecture -eq 'Arm64' -and -not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '1' }
    if ($Architecture -eq 'arm64') {
        $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '1'
        $env:CARGO_PROFILE_RELEASE_LTO = 'false'
        $env:CARGO_PROFILE_RELEASE_STRIP = 'false'
        $env:CARGO_INCREMENTAL = '0'
    }
    & (Join-Path $PSScriptRoot 'fetch-meowcal-core.ps1') -Architecture $Architecture
    $coreDirectory = Join-Path $repositoryRoot 'src-tauri\resources\core'
    cargo test --locked --manifest-path src-tauri/Cargo.toml --target $target --features store --lib
    if ($LASTEXITCODE -ne 0) { throw 'Store application tests failed.' }
    $previousCoreExecutable = $env:MEOWCAL_CORE_EXECUTABLE
    try {
        $env:MEOWCAL_CORE_EXECUTABLE = Join-Path $coreDirectory 'meowcal-core.exe'
        $handshake = @(cargo test --locked --manifest-path src-tauri/Cargo.toml --target $target --features store --lib core_client::tests::real_core_handshake_status_and_shutdown -- --ignored --exact 2>&1)
        $handshakeExit = $LASTEXITCODE
        $handshake | ForEach-Object { Write-Host $_ }
        if ($handshakeExit -ne 0 -or ($handshake -join "`n") -notmatch 'test result: ok\. 1 passed; 0 failed;') { throw 'Store package Core handshake failed.' }
    } finally { $env:MEOWCAL_CORE_EXECUTABLE = $previousCoreExecutable }

    $buildArguments = @('--target', $target, '--no-bundle', '--features', 'store', '--config', 'src-tauri/tauri.store.conf.json')
    if ($Configuration -eq 'Debug') { $buildArguments += '--debug' }
    npx --no -- tauri build @buildArguments
    if ($LASTEXITCODE -ne 0) { throw 'Store Tauri build failed.' }
    $mainExecutable = Join-Path $CargoTargetDir "$target\$($Configuration.ToLowerInvariant())\meowcal-sub.exe"
    Assert-StorePeArchitecture $mainExecutable $Architecture
    Assert-StorePeArchitecture (Join-Path $coreDirectory 'meowcal-core.exe') $Architecture

    $layout = Join-Path $OutputDirectory 'layout'
    New-Item -ItemType Directory -Path "$layout\Assets", "$layout\resources\core" -Force | Out-Null
    Copy-Item -LiteralPath $mainExecutable -Destination "$layout\meowcal-sub.exe"
    foreach ($file in @('meowcal-core.exe', 'meowcal-core.json', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $coreDirectory $file) -Destination "$layout\resources\core\$file"
    }
    foreach ($file in @('LICENSE', 'LICENSE-NOTICE.md')) { Copy-Item -LiteralPath $file -Destination $layout }
    foreach ($file in @('Square150x150Logo.png', 'Square44x44Logo.png', 'StoreLogo.png')) {
        Copy-Item -LiteralPath "src-tauri\icons\$file" -Destination "$layout\Assets\$file"
    }
    $displayName = if ($Configuration -eq 'Debug') { 'Meowcal Sub - Store Dev' } else { 'Meowcal Sub' }
    $manifest = New-StoreManifest $PackageName $Publisher $PublisherDisplayName $PackageVersion $Architecture $displayName
    [IO.File]::WriteAllText("$layout\AppxManifest.xml", $manifest, [Text.UTF8Encoding]::new($false))
    $package = Join-Path $OutputDirectory "MeowcalSub-$PackageVersion-$Architecture-$Configuration.msix"
    & $makeAppx pack /d $layout /p $package /o
    if ($LASTEXITCODE -ne 0) { throw 'MakeAppx validation or packaging failed.' }
    Get-FileHash -Algorithm SHA256 -LiteralPath $package | ConvertTo-Json | Set-Content "$OutputDirectory\SHA256.json"
    Write-Host "Unsigned Store package: $package"
    Write-Host 'Store identity and certification are required before publication. Debug packages are local validation only.'
} finally {
    Pop-Location
    foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
}
