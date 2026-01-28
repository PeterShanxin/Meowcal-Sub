# Build OverlayHost for distribution

Write-Host "Building OverlayHost..." -ForegroundColor Cyan

Push-Location src-winui3\OverlayHost

# Build Release version
dotnet publish -c Release -r win-x64 --self-contained false -p:PublishSingleFile=false

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Build failed" -ForegroundColor Red
    Pop-Location
    exit 1
}

Write-Host "✅ OverlayHost built successfully" -ForegroundColor Green

# Copy to tauri resources dir
$TargetDir = "..\..\src-tauri\resources"
if (!(Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir
}

Copy-Item "bin\Release\net9.0-windows10.0.22621.0\win-x64\publish\*" $TargetDir -Recurse -Force

Write-Host "✅ Copied to tauri resources" -ForegroundColor Green

Pop-Location
