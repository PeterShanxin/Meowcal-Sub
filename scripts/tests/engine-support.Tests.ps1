[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$supportScript = Join-Path $repositoryRoot "scripts\support-engine.ps1"
$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) (
    "meowcal-support-tests-" + [guid]::NewGuid().ToString("N")
)
$diagnosticsPath = Join-Path $temporaryDirectory "diagnostics.json"

try {
    New-Item -ItemType Directory -Path $temporaryDirectory -Force | Out-Null
    & pwsh -NoProfile -File $supportScript `
        -Action Diagnostics `
        -CacheDirectory $temporaryDirectory `
        -OutputPath $diagnosticsPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Diagnostics command failed." }
    $diagnostics = Get-Content -LiteralPath $diagnosticsPath -Raw | ConvertFrom-Json
    foreach ($property in @(
        "engineVersion",
        "architecture",
        "runtimeId",
        "windowsBuild",
        "totalRamBytes",
        "runtimeValid",
        "modelValid"
    )) {
        if ($null -eq $diagnostics.$property) {
            throw "Diagnostics output is missing '$property'."
        }
    }
    if ($diagnostics.runtimeValid -or $diagnostics.modelValid) {
        throw "Empty test cache must not be reported as installed."
    }
    Write-Host "Engine support script contract passed." -ForegroundColor Green
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
