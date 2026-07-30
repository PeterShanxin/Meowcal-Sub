[CmdletBinding()]
param(
    [ValidateSet("Diagnostics", "Verify", "InstallRepair", "CollectLogs")]
    [string]$Action = "Diagnostics",
    [string]$CacheDirectory = (Join-Path $env:LOCALAPPDATA "com.meowcal.sub"),
    [string]$OutputPath = "",
    [switch]$Unattended
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "config\engine-manifest.v1.json"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$engineRoot = Join-Path $CacheDirectory "meowcal-sub"

function Get-Runtime {
    $machineArchitecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    $architecture = switch ($machineArchitecture) {
        "ARM64" { "aarch64" }
        "AMD64" { "x86_64" }
        default { throw "ENGINE_UNSUPPORTED_ARCH: $machineArchitecture" }
    }
    $runtime = $manifest.runtimes | Where-Object architecture -eq $architecture | Select-Object -First 1
    if ($null -eq $runtime) {
        throw "ENGINE_UNSUPPORTED_ARCH: $machineArchitecture"
    }
    return $runtime
}

function Get-EnginePaths {
    param($Runtime)
    $runtimeRoot = Join-Path (Join-Path $engineRoot "runtime") $Runtime.installDirectory
    $modelRoot = Join-Path (Join-Path $engineRoot "models") $manifest.model.installDirectory
    return [pscustomobject]@{
        RuntimeArchive = Join-Path (Join-Path $engineRoot "runtime") $Runtime.archive.fileName
        RuntimeRoot = $runtimeRoot
        Executable = Join-Path $runtimeRoot $Runtime.executable.relativePath
        ModelRoot = $modelRoot
        Model = Join-Path $modelRoot $manifest.model.artifact.fileName
    }
}

function Test-Artifact {
    param([string]$Path, [long]$Size, [string]$Sha256)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -ne $Size) { return $false }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Sha256
}

function Restore-PendingAsset {
    param([string]$Path)
    $backup = "$Path.rollback"
    if (-not (Test-Path -LiteralPath $backup)) { return }
    Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
    Move-Item -LiteralPath $backup -Destination $Path
}

function Save-Artifact {
    param($Artifact, [string]$Destination, [string]$Label)
    $partial = "$Destination.download.part"
    New-Item -ItemType Directory -Path (Split-Path -Parent $Destination) -Force | Out-Null
    Write-Host "Downloading $Label..." -ForegroundColor Cyan
    Invoke-WebRequest -Uri $Artifact.url -OutFile $partial -Resume -MaximumRetryCount 3
    if (-not (Test-Artifact $partial $Artifact.sizeBytes $Artifact.sha256)) {
        throw "ENGINE_ARTIFACT_INTEGRITY: $Label"
    }
    Move-Item -LiteralPath $partial -Destination $Destination -Force
}

function Assert-Preflight {
    $os = Get-CimInstance Win32_OperatingSystem
    $computer = Get-CimInstance Win32_ComputerSystem
    $drive = [System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($engineRoot))
    if ([int]$os.BuildNumber -lt [int]$manifest.requirements.minimumWindowsBuild) {
        throw "ENGINE_INCOMPATIBLE: Windows build $($manifest.requirements.minimumWindowsBuild) required"
    }
    if ([uint64]$computer.TotalPhysicalMemory -lt [uint64]$manifest.requirements.minimumRamBytes) {
        throw "ENGINE_INCOMPATIBLE: insufficient RAM"
    }
    if ([uint64]$drive.AvailableFreeSpace -lt [uint64]$manifest.requirements.minimumFreeDiskBytes) {
        throw "ENGINE_DISK_SPACE: insufficient free storage"
    }
}

function Write-Diagnostics {
    $runtime = Get-Runtime
    $paths = Get-EnginePaths $runtime
    $os = Get-CimInstance Win32_OperatingSystem
    $computer = Get-CimInstance Win32_ComputerSystem
    $diagnostics = [ordered]@{
        timestamp = (Get-Date).ToUniversalTime().ToString("o")
        appVersion = $manifest.minimumAppVersion
        engineVersion = $manifest.engineVersion
        architecture = $runtime.architecture
        runtimeId = $runtime.id
        windowsBuild = [int]$os.BuildNumber
        totalRamBytes = [uint64]$computer.TotalPhysicalMemory
        engineRoot = $engineRoot
        runtimeValid = Test-Artifact $paths.Executable $runtime.executable.sizeBytes $runtime.executable.sha256
        modelValid = Test-Artifact $paths.Model $manifest.model.artifact.sizeBytes $manifest.model.artifact.sha256
    }
    $json = $diagnostics | ConvertTo-Json
    if ($OutputPath) {
        New-Item -ItemType Directory -Path (Split-Path -Parent $OutputPath) -Force | Out-Null
        Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8
    }
    $json
}

function Install-OrRepair {
    $runtime = Get-Runtime
    $paths = Get-EnginePaths $runtime
    Restore-PendingAsset $paths.RuntimeRoot
    Restore-PendingAsset $paths.Model
    $runtimeValid = Test-Artifact $paths.Executable $runtime.executable.sizeBytes $runtime.executable.sha256
    $modelValid = Test-Artifact $paths.Model $manifest.model.artifact.sizeBytes $manifest.model.artifact.sha256
    if (-not ($runtimeValid -and $modelValid)) { Assert-Preflight }
    New-Item -ItemType Directory -Path $paths.RuntimeRoot, $paths.ModelRoot -Force | Out-Null

    $runtimeCandidate = "$($paths.RuntimeRoot).candidate"
    $modelCandidate = "$($paths.Model).candidate"
    if (-not $runtimeValid) {
        if (-not (Test-Artifact $paths.RuntimeArchive $runtime.archive.sizeBytes $runtime.archive.sha256)) {
            Save-Artifact $runtime.archive $paths.RuntimeArchive "runtime"
        }
        Remove-Item -LiteralPath $runtimeCandidate -Recurse -Force -ErrorAction SilentlyContinue
        Expand-Archive -LiteralPath $paths.RuntimeArchive -DestinationPath $runtimeCandidate -Force
        $candidateExecutable = Join-Path $runtimeCandidate $runtime.executable.relativePath
        if (-not (Test-Artifact $candidateExecutable $runtime.executable.sizeBytes $runtime.executable.sha256)) {
            throw "ENGINE_ARTIFACT_INTEGRITY: runtime executable"
        }
    }
    if (-not $modelValid) {
        Save-Artifact $manifest.model.artifact $modelCandidate "Tencent HY-MT model"
    }

    $promotions = @()
    if (-not $runtimeValid) { $promotions += ,@($runtimeCandidate, $paths.RuntimeRoot) }
    if (-not $modelValid) { $promotions += ,@($modelCandidate, $paths.Model) }
    try {
        foreach ($promotion in $promotions) {
            $candidate, $target = $promotion
            $backup = "$target.rollback"
            Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $target) { Move-Item -LiteralPath $target -Destination $backup }
            Move-Item -LiteralPath $candidate -Destination $target
        }
        if (-not (Test-Artifact $paths.Executable $runtime.executable.sizeBytes $runtime.executable.sha256) -or
            -not (Test-Artifact $paths.Model $manifest.model.artifact.sizeBytes $manifest.model.artifact.sha256)) {
            throw "ENGINE_ARTIFACT_INTEGRITY: promoted engine"
        }
        foreach ($promotion in $promotions) {
            Remove-Item -LiteralPath "$($promotion[1]).rollback" -Recurse -Force -ErrorAction SilentlyContinue
        }
        Write-Host "Tencent HY-MT engine is installed and verified." -ForegroundColor Green
    } catch {
        if ($promotions.Count -gt 0) {
            foreach ($promotion in @($promotions)[($promotions.Count - 1)..0]) {
                $target = $promotion[1]
                $backup = "$target.rollback"
                Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction SilentlyContinue
                if (Test-Path -LiteralPath $backup) { Move-Item -LiteralPath $backup -Destination $target }
            }
        }
        throw
    }
}

function Collect-Logs {
    $destination = if ($OutputPath) { $OutputPath } else {
        Join-Path ([System.IO.Path]::GetTempPath()) ("meowcal-support-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".zip")
    }
    $staging = Join-Path ([System.IO.Path]::GetTempPath()) ("meowcal-support-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    try {
        $diagnosticsPath = Join-Path $staging "diagnostics.json"
        & $PSCommandPath -Action Diagnostics -CacheDirectory $CacheDirectory -OutputPath $diagnosticsPath | Out-Null
        Get-ChildItem -LiteralPath $engineRoot -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object Name -Match '\.(log|json)$' |
            Copy-Item -Destination $staging -Force
        Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $destination -Force
        Write-Host "Support bundle: $destination" -ForegroundColor Green
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not $Unattended -and $Action -eq "InstallRepair") {
    Write-Host "Installing the curated Tencent HY-MT engine into $engineRoot"
}
switch ($Action) {
    "Diagnostics" { Write-Diagnostics }
    "Verify" {
        $runtime = Get-Runtime
        $paths = Get-EnginePaths $runtime
        if (-not (Test-Artifact $paths.Executable $runtime.executable.sizeBytes $runtime.executable.sha256) -or
            -not (Test-Artifact $paths.Model $manifest.model.artifact.sizeBytes $manifest.model.artifact.sha256)) {
            throw "ENGINE_ARTIFACT_INTEGRITY: install or repair is required"
        }
        Write-Host "Tencent HY-MT engine integrity verified." -ForegroundColor Green
    }
    "InstallRepair" { Install-OrRepair }
    "CollectLogs" { Collect-Logs }
}
