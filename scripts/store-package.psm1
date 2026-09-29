Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-StoreIdentity {
    param([string]$Name, [string]$Publisher, [string]$PublisherDisplayName, [string]$Version)
    if ($Name -notmatch '^[A-Za-z0-9.-]{3,50}$') { throw 'Package identity name must contain 3-50 letters, digits, dots or hyphens.' }
    if ([string]::IsNullOrWhiteSpace($PublisherDisplayName)) { throw 'PublisherDisplayName is required.' }
    if ($Publisher -notmatch '^CN=') { throw 'Publisher must be the complete CN= identity from Partner Center.' }
    try { $null = [Security.Cryptography.X509Certificates.X500DistinguishedName]::new($Publisher) }
    catch { throw 'Publisher is not a valid X.500 distinguished name.' }
    if ($Version -notmatch '^[1-9][0-9]*\.[0-9]+\.[0-9]+\.0$') {
        throw 'PackageVersion must be major.minor.build.0 with a nonzero major; Store owns the fourth component.'
    }
    foreach ($part in $Version.Split('.')) {
        $number = 0
        if (-not [int]::TryParse($part, [ref]$number) -or $number -gt 65535) {
            throw 'Each package version component must be between 0 and 65535.'
        }
    }
}

function Assert-StorePeArchitecture {
    param([string]$Path, [ValidateSet('x64', 'arm64')][string]$Architecture)
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Not a PE executable: $Path" }
        $stream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        if ($offset -gt $stream.Length - 6) { throw "Invalid PE header: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550) { throw "Invalid PE signature: $Path" }
        $expected = if ($Architecture -eq 'arm64') { 0xaa64 } else { 0x8664 }
        if ($reader.ReadUInt16() -ne $expected) { throw "PE architecture does not match ${Architecture}: $Path" }
    } finally { $stream.Dispose() }
}

function New-StoreManifest {
    param(
        [string]$Name, [string]$Publisher, [string]$PublisherDisplayName, [string]$Version,
        [ValidateSet('x64', 'arm64')][string]$Architecture,
        [string]$DisplayName = 'Meowcal Sub'
    )
    Assert-StoreIdentity $Name $Publisher $PublisherDisplayName $Version
    $publisherXml = [Security.SecurityElement]::Escape($Publisher)
    $publisherDisplayXml = [Security.SecurityElement]::Escape($PublisherDisplayName)
    $displayXml = [Security.SecurityElement]::Escape($DisplayName)
    return @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
 xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
 xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
 xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
 IgnorableNamespaces="uap uap10 rescap">
 <Identity Name="$Name" Publisher="$publisherXml" Version="$Version" ProcessorArchitecture="$Architecture" />
 <Properties>
  <DisplayName>$displayXml</DisplayName>
  <PublisherDisplayName>$publisherDisplayXml</PublisherDisplayName>
  <Description>On-device subtitle capture and translation for Windows</Description>
  <Logo>Assets\StoreLogo.png</Logo>
 </Properties>
 <Resources><Resource Language="en-us" /></Resources>
 <Dependencies>
  <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.26200.0" />
  <PackageDependency Name="Microsoft.VCLibs.140.00.UWPDesktop" MinVersion="14.0.33728.0" Publisher="CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US" />
 </Dependencies>
 <Applications>
  <Application Id="App" Executable="meowcal-sub.exe" uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">
   <uap:VisualElements DisplayName="$displayXml" Description="On-device subtitle translation" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" BackgroundColor="transparent" />
  </Application>
 </Applications>
 <Capabilities><rescap:Capability Name="runFullTrust" /></Capabilities>
</Package>
"@
}

function Get-StoreMakeAppx {
    $sdkRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction Stop).KitsRoot10
    $hostArchitecture = if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
    $tool = Get-ChildItem (Join-Path $sdkRoot 'bin') -Directory |
        Where-Object { $_.Name -match '^10\.0\.\d+\.0$' } |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName "$hostArchitecture\MakeAppx.exe" } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $tool) { throw 'Windows SDK MakeAppx.exe is required.' }
    return $tool
}

Export-ModuleMember -Function Assert-StoreIdentity, Assert-StorePeArchitecture, New-StoreManifest, Get-StoreMakeAppx
