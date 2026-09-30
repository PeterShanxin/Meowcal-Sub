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
