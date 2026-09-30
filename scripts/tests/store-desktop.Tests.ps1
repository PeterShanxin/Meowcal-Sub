$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 'prepare-store-desktop.ps1'
$testOutput = Join-Path ([IO.Path]::GetTempPath()) ('store-desktop-guard-' + [guid]::NewGuid().ToString('N'))
$originalActions = $env:GITHUB_ACTIONS
$originalEnvironment = $env:RUNNER_ENVIRONMENT
try {
    foreach ($context in @(@{actions='false';environment='github-hosted'}, @{actions='true';environment='self-hosted'})) {
        $env:GITHUB_ACTIONS = $context.actions
        $env:RUNNER_ENVIRONMENT = $context.environment
        $rejected = $false
        try { & $scriptPath -OutputDirectory $testOutput }
        catch {
            if ($_.Exception.Message -notlike '*restricted to disposable GitHub-hosted runners*') { throw }
            $rejected = $true
        }
        if (-not $rejected) { throw 'Desktop preparation accepted a non-disposable context.' }
        if (Test-Path -LiteralPath $testOutput) { throw 'Desktop preparation wrote output before rejecting its context.' }
    }
} finally {
    $env:GITHUB_ACTIONS = $originalActions
    $env:RUNNER_ENVIRONMENT = $originalEnvironment
}
Write-Host 'Hosted desktop preparation guards passed.'
