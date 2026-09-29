[CmdletBinding()]
param([Parameter(Mandatory)][string]$PackageDirectory, [Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Test signing is restricted to disposable GitHub-hosted runners.'
}
if (Test-Path $OutputDirectory) { throw 'Fixture directory must be new.' }
New-Item -ItemType Directory $OutputDirectory | Out-Null
Import-Module (Join-Path $PSScriptRoot '../store-package.psm1') -Force
$makeAppx = Get-StoreMakeAppx
$signTool = Join-Path (Split-Path $makeAppx) 'signtool.exe'
$layout = Join-Path $OutputDirectory 'upgrade-layout'
Copy-Item (Join-Path $PackageDirectory 'layout') $layout -Recurse
$manifestPath = Join-Path $layout 'AppxManifest.xml'
[xml]$manifest = Get-Content $manifestPath
if ($manifest.Package.Identity.Name -ne 'MeowcalSub.StoreCITest' -or
    $manifest.Package.Identity.Publisher -ne 'CN=Meowcal Sub CI Validation') { throw 'Unexpected test identity.' }
$architecture = $manifest.Package.Identity.ProcessorArchitecture
Copy-Item (Join-Path $PackageDirectory "MeowcalSub-1.0.0.0-$architecture-Release.msix") "$OutputDirectory/initial.msix"
$manifest.Package.Identity.Version = '1.0.1.0'
$manifest.Save($manifestPath)
& $makeAppx pack /d $layout /p "$OutputDirectory/upgrade.msix" /o
if ($LASTEXITCODE) { throw 'Upgrade packaging failed.' }
$certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Meowcal Sub CI Validation' -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddDays(2)
try {
    Export-Certificate -Cert $certificate -FilePath "$OutputDirectory/local-test.cer" | Out-Null
    foreach ($file in @('initial.msix','upgrade.msix')) {
        & $signTool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My "$OutputDirectory/$file"
        if ($LASTEXITCODE) { throw "Signing failed: $file" }
    }
} finally { Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($certificate.Thumbprint)" }
Get-FileHash "$OutputDirectory/initial.msix","$OutputDirectory/upgrade.msix" | ConvertTo-Json | Set-Content "$OutputDirectory/packages.json"
$archive = Join-Path $OutputDirectory 'dependencies.zip'
Invoke-WebRequest 'https://github.com/microsoft/winget-cli/releases/download/v1.29.380/DesktopAppInstaller_Dependencies.zip' -OutFile $archive
if ((Get-FileHash $archive).Hash -ne 'BA875AFE9D190F61218985AC0292A99D1DB710BF93E13C68944CA9D89F0D82D1') { throw 'Microsoft dependency archive hash mismatch.' }
Expand-Archive $archive "$OutputDirectory/dependencies"
$framework = Get-ChildItem "$OutputDirectory/dependencies" -Recurse -Filter "Microsoft.VCLibs.140.00.UWPDesktop_14.0.33728.0_$architecture.appx" | Select-Object -First 1
if (-not $framework) { throw 'Desktop framework missing from pinned archive.' }
& $signTool verify /pa $framework.FullName
if ($LASTEXITCODE) { throw 'Desktop framework signature failed.' }
Copy-Item $framework.FullName "$OutputDirectory/vclibs.appx"
