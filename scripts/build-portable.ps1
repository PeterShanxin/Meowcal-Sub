[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$TargetTriple,
    [Parameter(Mandatory)][ValidateSet('x64', 'arm64')][string]$Architecture,
    [Parameter(Mandatory)][string]$CargoTargetDir
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'portable-runtime.psm1') -Force
$repo = Split-Path -Parent $PSScriptRoot
$tauri = Join-Path $repo 'src-tauri'
$version = (Get-Content -LiteralPath (Join-Path $tauri 'tauri.conf.json') -Raw | ConvertFrom-Json).version
$out = Join-Path $CargoTargetDir "$TargetTriple\release"
$stage = Join-Path $CargoTargetDir ('portable-stage-' + [guid]::NewGuid())
$smoke = Join-Path $CargoTargetDir ('portable-smoke-' + [guid]::NewGuid())
$zipDir = Join-Path $out 'bundle\portable'
New-Item -ItemType Directory -Path $zipDir, $stage, $smoke -Force | Out-Null
$zip = Join-Path $zipDir "Meowcal.Sub_${version}_${Architecture}-portable.zip"

Push-Location $repo
try {
    npx --no -- tauri build --target $TargetTriple --config src-tauri/tauri.portable.conf.json --features portable --no-bundle -- --locked
    if ($LASTEXITCODE -ne 0) { throw "Portable build failed for $Architecture." }
    Copy-Item -LiteralPath (Join-Path $out 'meowcal-sub.exe') -Destination (Join-Path $stage 'Meowcal Sub.exe')
    Copy-Item -LiteralPath (Join-Path $tauri 'resources') -Destination $stage -Recurse
    Copy-Item -LiteralPath (Join-Path $repo 'licenses') -Destination $stage -Recurse
    Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $stage
    Copy-PortableCrt -Architecture $Architecture -Directory $stage
    @'
Meowcal Sub portable

Extract this ZIP to a normal folder and open Meowcal Sub.exe.
Requires Windows 11 with Microsoft Edge WebView2 Runtime.
To update, close the app, extract the new ZIP to a fresh folder, and open it.
Settings and engine data stay in a dedicated Windows profile, not the ZIP folder.
The included Microsoft Visual C++ runtime is distributed under Microsoft's license terms.
'@ | Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Encoding utf8NoBOM
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
    Expand-Archive -LiteralPath $zip -DestinationPath $smoke
    & (Join-Path $PSScriptRoot 'test-portable-startup.ps1') -Directory $smoke
} finally {
    Pop-Location
    foreach ($dir in @($stage, $smoke)) {
        $resolved = (Resolve-Path -LiteralPath $dir).ProviderPath
        $root = (Resolve-Path -LiteralPath $CargoTargetDir).ProviderPath
        if (!$resolved.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Cleanup path escaped packaging output: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
