function Invoke-PackageBuildStep {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Action,
        [Parameter(Mandatory)][AllowEmptyCollection()][Collections.Generic.List[object]]$Steps
    )
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $succeeded = $false
    Write-Host "==> $Name"
    try {
        & $Action
        $succeeded = $true
    } finally {
        $timer.Stop()
        $Steps.Add([ordered]@{
            name = $Name
            succeeded = $succeeded
            seconds = [math]::Round($timer.Elapsed.TotalSeconds, 3)
        })
        Write-Host ("{0}: {1:N1}s (success={2})" -f $Name, $timer.Elapsed.TotalSeconds, $succeeded)
    }
}

function Write-PackageBuildReport {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Architecture,
        [Parameter(Mandatory)][DateTimeOffset]$StartedAt,
        [Parameter(Mandatory)][bool]$Succeeded,
        [Parameter(Mandatory)][AllowEmptyCollection()][Collections.Generic.List[object]]$Steps
    )
    $finishedAt = [DateTimeOffset]::UtcNow
    $report = [ordered]@{
        schemaVersion = 1
        commit = $env:GITHUB_SHA
        architecture = $Architecture
        hostArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        os = [Environment]::OSVersion.VersionString
        processors = [Environment]::ProcessorCount
        cargoBuildJobs = $env:CARGO_BUILD_JOBS
        startedAtUtc = $StartedAt.ToString('o')
        finishedAtUtc = $finishedAt.ToString('o')
        seconds = [math]::Round(($finishedAt - $StartedAt).TotalSeconds, 3)
        succeeded = $Succeeded
        steps = @($Steps.ToArray())
    }
    New-Item -ItemType Directory -Path (Split-Path $Path -Parent) -Force | Out-Null
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Path -Encoding utf8
    Write-Host "Build report: $Path"
}

Export-ModuleMember -Function Invoke-PackageBuildStep, Write-PackageBuildReport
