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
. (Join-Path $PSScriptRoot 'store-desktop-target.ps1')
$windowsDirectory = 'C:\Windows'
$privacy = @{name='WWAHost';path='C:\Windows\System32\WWAHost.exe';family='Microsoft.Windows.CloudExperienceHost_test'}
if (-not (Test-StoreDesktopInputTarget $privacy '{ENTER}' $windowsDirectory)) { throw 'Verified privacy host rejected.' }
foreach ($invalid in @(
    @{name='WWAHost';path='C:\Windows\System32\WWAHost.exe';family='Other.App_test'},
    @{name='WWAHost';path='C:\Other\WWAHost.exe';family='Microsoft.Windows.CloudExperienceHost_test'},
    @{name='Other';path='C:\Windows\System32\WWAHost.exe';family='Microsoft.Windows.CloudExperienceHost_test'},
    @{name='explorer';path='C:\Windows\explorer.exe';family=''}
)) {
    if (Test-StoreDesktopInputTarget $invalid '{ENTER}' $windowsDirectory) { throw 'Unverified input target accepted.' }
}
$startMenu = @{name='StartMenuExperienceHost';path='C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_test\StartMenuExperienceHost.exe';family='MicrosoftWindows.Client.CBS_test'}
if (Test-StoreDesktopInputTarget $startMenu '{ENTER}' $windowsDirectory) { throw 'Enter accepted for Start menu.' }
if (-not (Test-StoreDesktopInputTarget $startMenu '{ESC}' $windowsDirectory)) { throw 'Escape rejected for verified Start menu.' }
Write-Host 'Hosted desktop input targeting tests passed.'
