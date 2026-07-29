[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$resourceDirectory = Join-Path $repositoryRoot "src-tauri\resources"
$requiredFiles = @(
    "OverlayHost.exe",
    "placeholder.dll",
    "placeholder.pri"
)

New-Item -ItemType Directory -Force -Path $resourceDirectory | Out-Null

foreach ($fileName in $requiredFiles) {
    $path = Join-Path $resourceDirectory $fileName
    if (-not (Test-Path -LiteralPath $path)) {
        New-Item -ItemType File -Path $path | Out-Null
        Write-Host "Created validation placeholder: $path"
    }
}

Write-Host "Tauri validation resources are ready."
Write-Host "For development or packaging, run scripts\build-overlayhost.ps1."
