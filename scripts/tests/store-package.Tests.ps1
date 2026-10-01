$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '..\store-package.psm1') -Force
function Assert-Throws([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Message*") { throw }
        return
    }
    throw "Expected rejection: $Message"
}
foreach ($architecture in @('x64', 'arm64')) {
    [xml]$manifest = New-StoreManifest 'MeowcalSub.Test' 'CN=Local Validation' 'A & B' '1.2.3.0' $architecture
    if ($manifest.Package.Identity.ProcessorArchitecture -ne $architecture -or
        $manifest.Package.Identity.Version -ne '1.2.3.0' -or
        $manifest.Package.Properties.PublisherDisplayName -ne 'A & B' -or
        $manifest.Package.Applications.Application.Executable -ne 'meowcal-sub.exe') { throw 'Manifest values were not preserved.' }
    if ($manifest.Package.Capabilities.Capability.Name -ne 'runFullTrust') { throw 'Desktop application capability missing.' }
    $runtime = $manifest.Package.Dependencies.PackageDependency
    if ($runtime.Name -ne 'Microsoft.VCLibs.140.00.UWPDesktop' -or
        $runtime.Publisher -ne 'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US' -or
        [version]$runtime.MinVersion -lt [version]'14.0.33728.0') {
        throw 'Desktop C++ runtime dependency missing or unsupported.'
    }
}
Assert-Throws { Assert-StoreIdentity '../invalid' 'CN=Test' 'Test' '1.0.0.0' } 'identity name'
Assert-Throws { Assert-StoreIdentity 'MeowcalSub.Test' 'invalid' 'Test' '1.0.0.0' } 'Publisher'
Assert-Throws { Assert-StoreIdentity 'MeowcalSub.Test' 'CN=Test' '' '1.0.0.0' } 'PublisherDisplayName'
foreach ($version in @('0.8.6.0', '1.0.0.1', '1.65536.0.0', '1.2.3', '1.2.3.0"')) {
    Assert-Throws { Assert-StoreIdentity 'MeowcalSub.Test' 'CN=Test' 'Test' $version } 'version'
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('meowcal-store-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    $file = Join-Path $testRoot 'fixture.exe'
    $bytes = [byte[]]::new(256)
    [BitConverter]::GetBytes([uint16]0x5a4d).CopyTo($bytes, 0)
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes, 0x80)
    [BitConverter]::GetBytes([uint16]0xaa64).CopyTo($bytes, 0x84)
    [IO.File]::WriteAllBytes($file, $bytes)
    Assert-StorePeArchitecture $file 'arm64'
    Assert-Throws { Assert-StorePeArchitecture $file 'x64' } 'architecture'
    [IO.File]::WriteAllBytes($file, [byte[]]@(0, 1))
    Assert-Throws { Assert-StorePeArchitecture $file 'arm64' } 'Not a PE'
} finally {
    Remove-Item -LiteralPath $file -Force
    Remove-Item -LiteralPath $testRoot -Force
}
Write-Host 'Store identity, manifest and PE architecture contracts passed.'
& (Join-Path $PSScriptRoot 'store-direct-diagnostics.Tests.ps1')

$assetRoot = Join-Path ([IO.Path]::GetTempPath()) ('meowcal-release-assets-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $assetRoot | Out-Null
try {
    foreach ($architecture in @('x64', 'arm64')) {
        foreach ($name in @("App-$architecture.msi", "App-$architecture-setup.exe", "App-$architecture-setup.exe.sig", "MeowcalSub-1.2.3.0-$architecture-Release.msix")) {
            [IO.File]::WriteAllText((Join-Path $assetRoot $name), "fixture $name")
        }
    }
    $checksum = Join-Path $assetRoot 'SHA256SUMS.txt'
    $verify = Join-Path $PSScriptRoot '..\verify-release-assets.ps1'
    & $verify -Directory $assetRoot -ChecksumPath $checksum -StorePackageVersion '1.2.3.0'
    $lines = @(Get-Content -LiteralPath $checksum)
    if ($lines.Count -ne 6) { throw 'Release checksums must cover four direct installers and two MSIX packages.' }
    foreach ($architecture in @('x64', 'arm64')) {
        $name = "MeowcalSub-1.2.3.0-$architecture-Release.msix"
        $hash = (Get-FileHash -LiteralPath (Join-Path $assetRoot $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($lines -notcontains "$hash  $name") { throw "Missing or incorrect Store checksum: $name" }
    }
    Assert-Throws { & $verify -Directory $assetRoot -ChecksumPath $checksum -StorePackageVersion '1.2.4.0' } 'Expected exactly one Store package'
    $armPackage = Join-Path $assetRoot 'MeowcalSub-1.2.3.0-arm64-Release.msix'
    Remove-Item -LiteralPath $armPackage
    Assert-Throws { & $verify -Directory $assetRoot -ChecksumPath $checksum -StorePackageVersion '1.2.3.0' } 'exactly two Store'
    [IO.File]::WriteAllText($armPackage, 'restored fixture')
    [IO.File]::WriteAllText((Join-Path $assetRoot 'stale.msix'), 'stale fixture')
    Assert-Throws { & $verify -Directory $assetRoot -ChecksumPath $checksum -StorePackageVersion '1.2.3.0' } 'exactly two Store'
    Remove-Item -LiteralPath (Join-Path $assetRoot 'stale.msix')
    & $verify -Directory $assetRoot -ChecksumPath $checksum
    if (@(Get-Content -LiteralPath $checksum).Count -ne 4) { throw 'Direct-only checksum compatibility changed.' }
} finally {
    Get-ChildItem -LiteralPath $assetRoot -File | Remove-Item -Force
    Remove-Item -LiteralPath $assetRoot -Force
}
Write-Host 'Store release asset and checksum contracts passed.'
