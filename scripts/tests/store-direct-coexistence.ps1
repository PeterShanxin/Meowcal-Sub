if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Direct-install coexistence validation is restricted to disposable hosted runners.'
}
function Install-DirectBaseline {
    $script:directConfig = Join-Path $env:APPDATA 'com.meowcal.sub\config.json'
    if (Test-Path $script:directConfig) { throw 'Direct-install validation requires a fresh user profile.' }
    $root = Join-Path $env:RUNNER_TEMP 'store-direct-baseline'
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
    $script:appPid = (Start-Process -FilePath $script:directExecutable -PassThru -WindowStyle Hidden).Id
    $deadline = (Get-Date).AddSeconds(40)
    do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
    $settings = Invoke-AppScript "(async()=>{const s=await window.__TAURI__.core.invoke('get_settings');s.sourceLanguage='ja-JP';await window.__TAURI__.core.invoke('save_settings',{settings:s});return s})()"
    if ($settings.sourceLanguage -ne 'ja-JP') { throw 'Direct baseline did not retain its language setting.' }
    Stop-TestApp -ExecutablePath $script:directExecutable
    $script:directConfigHash = (Get-FileHash $script:directConfig).Hash
    @{version='0.8.6';installerHash=$installerHash;executableHash=$script:directExecutableHash;configurationHash=$script:directConfigHash;startupVerified=$true}
}
function Test-DirectBaselineAfterStoreRemoval {
    if ((Get-FileHash $script:directExecutable).Hash -ne $script:directExecutableHash -or
        (Get-FileHash $script:directConfig).Hash -ne $script:directConfigHash) {
        throw 'Store lifecycle changed the direct installation or configuration.'
    }
    $script:appPid = (Start-Process -FilePath $script:directExecutable -PassThru -WindowStyle Hidden).Id
    $deadline = (Get-Date).AddSeconds(40)
    do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
    $settings = Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')"
    if ($settings.sourceLanguage -ne 'ja-JP') { throw 'Direct application lost its configuration after Store removal.' }
    Stop-TestApp -ExecutablePath $script:directExecutable
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
