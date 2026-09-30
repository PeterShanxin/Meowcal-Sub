if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Direct-install coexistence validation is restricted to disposable hosted runners.'
}
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'benchmark-webview.ps1')

function Write-DirectDiagnostic([string]$Stage, $Settings = $null) {
    $row = [ordered]@{stage=$Stage;time=(Get-Date).ToString('o');configPath=$script:directConfig;configExists=Test-Path $script:directConfig}
    $row.executableHash = (Get-FileHash -LiteralPath $script:directExecutable).Hash
    if ($row.configExists) {
        $row.configHash = (Get-FileHash -LiteralPath $script:directConfig).Hash
        try {
            $config = Get-Content -LiteralPath $script:directConfig -Raw | ConvertFrom-Json
            $row.configSourceLanguage = $config.sourceLanguage
        } catch { $row.configParseFailed = $true }
    }
    if ($PSBoundParameters.ContainsKey('Settings')) {
        $row.settingsSourceLanguage = $Settings.sourceLanguage
        $row.settingsHasSourceLanguage = [bool]($Settings -and $null -ne $Settings.PSObject.Properties['sourceLanguage'])
        $row.settingsResultType = if ($null -eq $Settings) { 'null' } else { $Settings.GetType().Name }
    }
    if ($script:directProcess) {
        $row.app = @{pid=$script:directProcess.Id;startTime=$script:directProcess.StartTime.ToString('o');exited=$script:directProcess.HasExited;executable=$script:directExecutable}
        if (-not $script:directProcess.HasExited) {
            Assert-BenchmarkEndpoint $script:directProcess
            $row.endpoint = @{address='127.0.0.1:9241';verified=$true;listeners=@(Get-NetTCPConnection -LocalPort 9241 -State Listen | Select-Object -ExpandProperty OwningProcess -Unique)}
        }
    }
    # Raw logs stay outside the uploaded output directory. Publish only fixed event names.
    $row.startupEvents = @(foreach ($log in Get-ChildItem $script:directLogDirectory -Filter '*.log' -ErrorAction SilentlyContinue) {
        $lineNumber = 0
        foreach ($line in Get-Content -LiteralPath $log.FullName) {
            $lineNumber++
            foreach ($event in @('Getting settings','Saving settings','No config found; starting with defaults','Config directory unavailable','restored the last known-good copy','using the last known-good copy for this session','no backup was usable; starting with defaults','Backup could not be restored','Could not persist the engine migration record','Failed to save settings')) {
                if ($line.Contains($event)) { @{file=$log.Name;line=$lineNumber;event=$event} }
            }
        }
    })
    $row | ConvertTo-Json -Depth 6 -Compress | Add-Content -LiteralPath "$output/direct-coexistence.jsonl" -Encoding UTF8
}

function Start-DirectApp {
    $previousLogDirectory = $env:MEOWCAL_LOG_DIR
    $previousLogFilter = $env:MEOWCAL_LOG_FILTER
    try {
        $env:MEOWCAL_LOG_DIR = $script:directLogDirectory
        $env:MEOWCAL_LOG_FILTER = 'meowcal_sub::config_store=debug,meowcal_sub::config_save=debug,meowcal_sub::settings_service=info'
        $script:directProcess = Start-Process -FilePath $script:directExecutable -PassThru -WindowStyle Hidden
        $null = $script:directProcess.Handle
        $script:appPid = $script:directProcess.Id
    } finally {
        $env:MEOWCAL_LOG_DIR = $previousLogDirectory
        $env:MEOWCAL_LOG_FILTER = $previousLogFilter
    }
    $deadline = (Get-Date).AddSeconds(40)
    do {
        try {
            if ($script:socket) { $script:socket.Dispose() }
            Connect-BenchmarkWebView $script:directProcess
            break
        } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 }
    } while ($true)
}

function Install-DirectBaseline {
    $script:directConfig = Join-Path $env:APPDATA 'com.meowcal.sub\config.json'
    if (Test-Path $script:directConfig) { throw 'Direct-install validation requires a fresh user profile.' }
    $root = Join-Path $env:RUNNER_TEMP 'store-direct-baseline'
    $script:directLogDirectory = Join-Path $root 'private-startup-logs'
    New-Item -ItemType Directory -Path $root | Out-Null
    $installer = Join-Path $root 'setup.exe'
    Invoke-WebRequest 'https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.8.6/Meowcal.Sub_0.8.6_arm64-setup.exe' -OutFile $installer
    $installerHash = (Get-FileHash $installer).Hash
    if ($installerHash -ne '097CBF9AA06E9DD86ACE344F22F8CCAC153E612EA34F8E69F17046A98AB8916F') { throw 'Direct installer hash mismatch.' }
    $script:directDirectory = Join-Path $root 'installed'
    $install = Start-Process -FilePath $installer -ArgumentList "/S /D=$script:directDirectory" -WindowStyle Hidden -PassThru
    $null = $install.Handle
    if (-not $install.WaitForExit(120000)) { Stop-Process -Id $install.Id; throw 'Direct installer timed out.' }
    if ($install.ExitCode -ne 0) { throw "Direct installer failed: $($install.ExitCode)" }
    $script:directExecutable = Join-Path $script:directDirectory 'meowcal-sub.exe'
    $script:directExecutableHash = (Get-FileHash $script:directExecutable).Hash
    Start-DirectApp
    Write-DirectDiagnostic 'before-save' (Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')")
    $settings = Invoke-AppScript "(async()=>{const s=await window.__TAURI__.core.invoke('get_settings');s.sourceLanguage='ja-JP';await window.__TAURI__.core.invoke('save_settings',{settings:s});return s})()"
    if ($settings.sourceLanguage -ne 'ja-JP') { throw 'Direct baseline did not retain its language setting.' }
    Write-DirectDiagnostic 'after-save-readback' (Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')")
    Stop-TestApp -ExecutablePath $script:directExecutable
    Write-DirectDiagnostic 'after-direct-close'
    $script:directProcess.Dispose()
    $script:directProcess = $null
    $script:directConfigHash = (Get-FileHash $script:directConfig).Hash
    @{version='0.8.6';installerHash=$installerHash;executableHash=$script:directExecutableHash;configurationHash=$script:directConfigHash;startupVerified=$true}
}
function Test-DirectBaselineAfterStoreRemoval {
    Write-DirectDiagnostic 'after-store-uninstall-before-relaunch'
    if ((Get-FileHash $script:directExecutable).Hash -ne $script:directExecutableHash -or
        (Get-FileHash $script:directConfig).Hash -ne $script:directConfigHash) {
        throw 'Store lifecycle changed the direct installation or configuration.'
    }
    Start-DirectApp
    $settings = Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')"
    Write-DirectDiagnostic 'after-direct-relaunch' $settings
    if ($settings.sourceLanguage -ne 'ja-JP') { throw 'Direct application lost its configuration after Store removal.' }
    Stop-TestApp -ExecutablePath $script:directExecutable
    Write-DirectDiagnostic 'after-relaunched-direct-close'
    $script:directProcess.Dispose()
    $script:directProcess = $null
    $uninstaller = Join-Path $script:directDirectory 'uninstall.exe'
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -PassThru
    $null = $uninstall.Handle
    if (-not $uninstall.WaitForExit(120000)) { Stop-Process -Id $uninstall.Id; throw 'Direct uninstaller timed out.' }
    if ($uninstall.ExitCode -ne 0) { throw "Direct uninstaller failed: $($uninstall.ExitCode)" }
    $deadline = (Get-Date).AddSeconds(30)
    while (Test-Path $script:directExecutable) {
        if ((Get-Date) -gt $deadline) { throw 'Direct uninstaller retained the application executable.' }
        Start-Sleep 1
    }
    @{directFilesAndSettingsPreserved=$true;startupAfterStoreRemoval=$true;directBaselineUninstalled=$true;directTranslationTested=$false}
}
