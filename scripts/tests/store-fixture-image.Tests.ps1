$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'store-fixture-image.ps1')
$expected = [Drawing.Bitmap]::new(100,40)
$graphics = [Drawing.Graphics]::FromImage($expected)
try {
    $graphics.Clear([Drawing.Color]::Black)
    $graphics.FillRectangle([Drawing.Brushes]::White,40,18,20,4)
} finally { $graphics.Dispose() }
$captured = [Drawing.Bitmap]$expected.Clone()
try {
    if (-not [StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Unobscured rendering rejected.' }
    $captured.SetPixel(40,18,[Drawing.Color]::FromArgb(239,239,239))
    if (-not [StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Small rasterization difference rejected.' }
    $captured.SetPixel(40,18,[Drawing.Color]::Black)
    if ([StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Text mismatch above the text threshold accepted.' }
    $captured.SetPixel(40,18,[Drawing.Color]::White)
    for ($x=0; $x -lt 5; $x++) { $captured.SetPixel($x,0,[Drawing.Color]::Gray) }
    if ([StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Background mismatch above the image threshold accepted.' }
    $captured.Dispose()
    $captured = [Drawing.Bitmap]$expected.Clone()
    $graphics = [Drawing.Graphics]::FromImage($captured)
    try { $graphics.FillRectangle([Drawing.Brushes]::Gray,35,15,30,10) }
    finally { $graphics.Dispose() }
    if ([StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Interior obstruction with black corners accepted.' }
    $graphics = [Drawing.Graphics]::FromImage($captured)
    try { $graphics.Clear([Drawing.Color]::Black) }
    finally { $graphics.Dispose() }
    if ([StoreFixtureImage]::IsVisible($expected,$captured)) { throw 'Missing fixture text accepted.' }
    if ([StoreFixtureImage]::IsVisible($captured,$captured)) { throw 'Blank reference accepted.' }
} finally { $captured.Dispose(); $expected.Dispose() }
Write-Host 'Store fixture image visibility tests passed.'
