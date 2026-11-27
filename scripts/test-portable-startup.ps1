[CmdletBinding()]
param([Parameter(Mandatory)][string]$Directory)

$ErrorActionPreference = 'Stop'
foreach ($entry in @('Meowcal Sub.exe', 'resources\core\meowcal-core.exe', 'resources\core\meowcal-core.json', 'resources\core\LICENSE', 'README.txt', 'LICENSE')) {
    if (!(Test-Path -LiteralPath (Join-Path $Directory $entry) -PathType Leaf)) {
        throw "Incomplete portable ZIP: missing $entry."
    }
}

$psi = [Diagnostics.ProcessStartInfo]::new()
$psi.FileName = Join-Path $Directory 'Meowcal Sub.exe'
$psi.WorkingDirectory = $Directory
$psi.Environment['MEOWCAL_LOG_DIR'] = Join-Path $Directory 'smoke-logs'
$psi.Environment['MEOWCAL_LOG_FILTER'] = 'meowcal_sub=info'
$process = [Diagnostics.Process]::Start($psi)
try {
    $limit = [DateTime]::UtcNow.AddSeconds(40)
    while ([DateTime]::UtcNow -lt $limit) {
        if ($process.HasExited) { throw "Extracted portable app exited during startup: $($process.ExitCode)." }
        $logs = @(Get-ChildItem -LiteralPath $psi.Environment['MEOWCAL_LOG_DIR'] -File -ErrorAction SilentlyContinue)
        if ($logs.Count -gt 0 -and (Select-String -LiteralPath $logs.FullName -SimpleMatch 'Portable workspace ready' -Quiet)) {
            Write-Host 'Extracted ZIP started the native workspace with the portable profile.' -ForegroundColor Green
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw 'The extracted portable app did not reach native workspace readiness.'
} finally {
    if (!$process.HasExited) {
        $null = $process.CloseMainWindow()
        if (!$process.WaitForExit(3000)) { $process.Kill($true); $process.WaitForExit() }
    }
    $process.Dispose()
}
