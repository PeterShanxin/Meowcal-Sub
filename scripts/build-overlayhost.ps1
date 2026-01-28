# Build OverlayHost for distribution
# Supports both x64 and ARM64 architectures

param(
    [ValidateSet("x64", "arm64", "auto")]
    [string]$Architecture = "auto"
)

Write-Host "Building OverlayHost..." -ForegroundColor Cyan

# Detect architecture if auto
if ($Architecture -eq "auto") {
    $procArch = $env:PROCESSOR_ARCHITECTURE
    if ($procArch -eq "ARM64") {
        $Architecture = "arm64"
    } else {
        $Architecture = "x64"
    }
    Write-Host "Auto-detected architecture: $Architecture" -ForegroundColor Yellow
}

# Map to dotnet RID
$RuntimeId = if ($Architecture -eq "arm64") { "win-arm64" } else { "win-x64" }

Push-Location src-winui3\OverlayHost

# Build Release version for the target architecture
Write-Host "Building for runtime: $RuntimeId" -ForegroundColor Cyan
dotnet publish -c Release -r $RuntimeId --self-contained false -p:PublishSingleFile=false

if ($LASTEXITCODE -ne 0) {
    Write-Host "X Build failed" -ForegroundColor Red
    Pop-Location
    exit 1
}

Write-Host "OK OverlayHost built successfully for $RuntimeId" -ForegroundColor Green

# Copy to tauri resources dir
$TargetDir = "..\..\src-tauri\resources"
if (!(Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir
}

$SourcePath = "bin\Release\net9.0-windows10.0.22621.0\$RuntimeId\publish\*"
Copy-Item $SourcePath $TargetDir -Recurse -Force

Write-Host "OK Copied to tauri resources" -ForegroundColor Green

Pop-Location
