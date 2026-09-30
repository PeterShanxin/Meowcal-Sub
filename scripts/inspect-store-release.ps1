param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][string]$StoreVersion)
$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/store-package.psm1" -Force
Add-Type -AssemblyName System.IO.Compression.FileSystem

foreach ($arch in @('x64', 'arm64')) {
    $name = "MeowcalSub-$StoreVersion-$arch-Release.msix"
    $archive = [IO.Compression.ZipFile]::OpenRead((Join-Path $Directory $name))
    try {
        $manifestEntry = $archive.GetEntry('AppxManifest.xml')
        if (-not $manifestEntry) { throw 'Missing MSIX manifest' }
        $settings = [Xml.XmlReaderSettings]::new()
        $settings.DtdProcessing = [Xml.DtdProcessing]::Prohibit
        $settings.XmlResolver = $null
        $xmlReader = [Xml.XmlReader]::Create($manifestEntry.Open(), $settings)
        try {
            $manifest = [xml]::new()
            $manifest.Load($xmlReader)
        } finally { $xmlReader.Dispose() }
        $identity = $manifest.Package.Identity
        if ($identity.Name -cne 'ShanxinLi.MeowcalSub' -or
            $identity.Publisher -cne 'CN=FCD37627-8F13-4157-933C-E729F9F08408' -or
            $identity.Version -ne $StoreVersion -or $identity.ProcessorArchitecture -ne $arch -or
            $manifest.Package.Properties.PublisherDisplayName -cne 'Shanxin Li' -or
            $manifest.Package.Dependencies.TargetDeviceFamily.MinVersion -ne '10.0.22000.0') {
            throw 'Store identity, version or target architecture mismatch'
        }
        if ($archive.GetEntry('AppxSignature.p7x')) { throw 'Expected unsigned Store package' }
        foreach ($exe in @('meowcal-sub.exe', 'resources/core/meowcal-core.exe')) {
            $entry = $archive.GetEntry($exe)
            if (-not $entry) { throw 'Missing application or Core executable' }
            $target = Join-Path $Directory "$arch-$([IO.Path]::GetFileName($exe))"
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
            Assert-StorePeArchitecture $target $arch
        }
        foreach ($icon in @('Square150x150Logo.png', 'Square44x44Logo.png', 'StoreLogo.png')) {
            $entry = $archive.GetEntry("Assets/$icon")
            if (-not $entry) { throw 'Missing canonical package icon' }
            $stream = $entry.Open()
            try { $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) }
            finally { $stream.Dispose() }
            if ($actual -ne (Get-FileHash (Join-Path $Directory $icon) -Algorithm SHA256).Hash) {
                throw 'Package icon differs from the release commit'
            }
        }
    } finally { $archive.Dispose() }
}
Write-Output 'Production Store identity, versions, PE architectures and icons verified for x64 and ARM64.'
