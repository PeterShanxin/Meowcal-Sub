param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][string]$StoreVersion)
$ErrorActionPreference = 'Stop'
$files = @(
    (Join-Path $Directory "MeowcalSub-$StoreVersion-x64-Release.msix"),
    (Join-Path $Directory "MeowcalSub-$StoreVersion-arm64-Release.msix"),
    (Join-Path $Directory 'store-listing-logo-300.png')
)
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'Missing staged submission file' }
}
Compress-Archive -LiteralPath $files -DestinationPath (Join-Path $Directory 'submission.zip') -CompressionLevel Optimal
