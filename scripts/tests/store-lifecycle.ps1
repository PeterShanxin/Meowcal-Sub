[CmdletBinding()]
param([Parameter(Mandatory)][string]$InputDirectory, [Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Signed lifecycle validation is restricted to disposable GitHub-hosted runners.'
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
$result = [ordered]@{started=(Get-Date).ToString('o');os=[Environment]::OSVersion.VersionString;architecture=$env:PROCESSOR_ARCHITECTURE;steps=@();complete=$false}
function Save-Result { $result | ConvertTo-Json -Depth 12 | Set-Content "$output/result.json" -Encoding UTF8 }
function Step([string]$Name, [scriptblock]$Action) {
    try { $value = & $Action; $result.steps += @{name=$Name;status='pass';details=$value}; Save-Result; return $value }
    catch { $result.steps += @{name=$Name;status='fail';error=$_.Exception.Message}; Save-Result; throw }
}
$existing=Get-AppxPackage MeowcalSub.StoreCITest
if ($existing) {
    $prefix=$existing.InstallLocation.TrimEnd('\')+'\'
    Get-Process | Where-Object {$_.Path -and $_.Path.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)} | Stop-Process
    Remove-AppxPackage $existing.PackageFullName
}
Start-Transcript "$output/transcript.txt" -Force
Save-Result
. (Join-Path $PSScriptRoot 'store-webview.ps1')
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport,Guid("2e941141-7f97-4756-ba1d-9decde894a3d"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IActivation { void ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string id,[MarshalAs(UnmanagedType.LPWStr)] string args,uint options,out uint pid); }
public static class Activation { public static uint Launch(string id) { var a=(IActivation)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C"))); uint pid; a.ActivateApplication(id,null,0,out pid); return pid; } }
'@
function Stop-TestApp {
    if ($script:socket -and $script:socket.State -eq [Net.WebSockets.WebSocketState]::Open) {
        try { $null = Invoke-AppScript "window.__TAURI__.core.invoke('prepare_for_update')" } catch { $_.Exception.Message | Add-Content "$output/stop-errors.txt" }
        $script:socket.Dispose()
    }
    if ($script:appPid) {
        $process = Get-Process -Id $script:appPid -ErrorAction SilentlyContinue
        if ($process -and $process.Path -eq (Join-Path $script:package.InstallLocation 'meowcal-sub.exe')) { Stop-Process -Id $script:appPid }
        $script:appPid = $null
    }
}
try {
    try { $result.defender = Get-MpComputerStatus | Select-Object AntivirusEnabled,RealTimeProtectionEnabled,BehaviorMonitorEnabled }
    catch { $result.defender = @{unavailable=$_.Exception.Message} }
    $result.webviewBefore = @(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*','HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue | Select-Object name,pv)
    $null = Step 'trust-test-certificate-on-runner' {
        $certificate = Import-Certificate -FilePath (Join-Path $InputDirectory 'local-test.cer') -CertStoreLocation Cert:\LocalMachine\TrustedPeople
        $script:thumbprint = $certificate.Thumbprint
        $certificate.Thumbprint
    }
    $null = Step 'signed-install' {
        Add-AppxPackage -Path (Join-Path $InputDirectory 'initial.msix') -DependencyPath (Join-Path $InputDirectory 'vclibs.appx')
        $script:package = Get-AppxPackage -Name MeowcalSub.StoreCITest
        if ($script:package.Version -ne '1.0.0.0') { throw 'Initial package version mismatch.' }
        $script:package | Select-Object PackageFullName,InstallLocation,Status,SignatureKind
    }
    $packageData = Join-Path $env:LOCALAPPDATA "Packages\$($script:package.PackageFamilyName)"
    $physicalCore = Join-Path $packageData 'LocalCache\com.meowcal.sub.store\Core'
    $result.coldCache = @{exists=Test-Path $physicalCore;inputContainsModel=@(Get-ChildItem $InputDirectory -Filter '*.gguf' -Recurse).Count -gt 0}
    Save-Result
    if ($result.coldCache.exists -or $result.coldCache.inputContainsModel) { throw 'Cold installation requires an empty engine cache.' }
    try {
        $null = Step 'release-activation-and-first-download' {
            $policy = 'HKLM:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
            New-Item $policy -Force | Out-Null
            New-ItemProperty $policy -Name '*' -Value '--remote-debugging-port=9241' -PropertyType String -Force | Out-Null
            $script:appPid = [Activation]::Launch($script:package.PackageFamilyName+'!App')
            $deadline = (Get-Date).AddSeconds(40)
            do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
            $settings = Invoke-AppScript "(async()=>{const s=await window.__TAURI__.core.invoke('get_settings');s.translation.localEngine.cpuOnly=true;s.sourceLanguage='en-US';s.targetLanguage='zh-CN';await window.__TAURI__.core.invoke('save_settings',{settings:s});return s})()"
            $settings | ConvertTo-Json -Depth 8 | Set-Content "$output/settings-before-install.json"
            $null = Invoke-AppScript "window.__install={done:false};window.__TAURI__.core.invoke('wizard_install_engine').then(()=>window.__install={done:true}).catch(e=>window.__install={done:true,error:String(e)});'started'"
            $deadline = (Get-Date).AddMinutes(20)
            do {
                Start-Sleep 5
                $state = Invoke-AppScript 'window.__install'
                @{time=(Get-Date).ToString('o');install=$state;files=@(Get-ChildItem $physicalCore -File -Recurse -ErrorAction SilentlyContinue | Select-Object Name,Length)} |
                    ConvertTo-Json -Depth 6 | Set-Content "$output/download-progress.json"
                if ((Get-Date) -gt $deadline) { throw 'First download exceeded 20 minutes.' }
            } until ($state.done)
            if ($state.error) { throw $state.error }
            $after = Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')"
            $after | ConvertTo-Json -Depth 8 | Set-Content "$output/settings-after-install.json"
            $runtime = $after.translation.localEngine.managedRuntime
            if (-not $runtime -or -not (Test-Path $runtime.modelPath)) { throw 'Install did not produce a registered model.' }
            $model = Get-FileHash -LiteralPath $runtime.modelPath
            if ($model.Hash -ne '4383AC0C3C8E476DE98FF979C2A3F069F8C4FB385E7860CF2D28DA896CC477C7') { throw 'Downloaded model hash mismatch.' }
            $sample = Invoke-AppScript "(async()=>{await window.__TAURI__.core.invoke('wizard_start_service');return await window.__TAURI__.core.invoke('wizard_test_translation',{sourceText:'Hello, how are you?',sourceLanguage:'en-US',targetLanguage:'zh-CN'})})()"
            if (-not $sample.translatedText -or $sample.translatedText -eq 'Hello, how are you?') { throw 'No translated output.' }
            $modules = @(Get-Process -Name llama-server -ErrorAction SilentlyContinue | ForEach-Object {
                @{path=$_.Path;runtimeDlls=@($_.Modules | Where-Object {$_.ModuleName -like 'vcruntime*'} | Select-Object ModuleName,FileName)}
            })
            $modules | ConvertTo-Json -Depth 6 | Set-Content "$output/inference-runtime-modules.json"
            @{modelHash=$model.Hash;sample=$sample;runtime=$runtime}
        }
    } catch { $_.Exception.Message | Set-Content "$output/cold-start-error.txt" }
    Stop-TestApp
    $null = Step 'upgrade-retains-package-data' {
        $sentinel = Join-Path $packageData 'LocalCache\store-upgrade-sentinel.txt'
        New-Item -ItemType Directory -Path (Split-Path $sentinel) -Force | Out-Null
        'store-upgrade-retention' | Set-Content $sentinel
        $before = (Get-FileHash $sentinel).Hash
        $configBefore = @(Get-ChildItem $packageData -Filter config.json -File -Recurse -ErrorAction SilentlyContinue | Get-FileHash)
        Add-AppxPackage -Path (Join-Path $InputDirectory 'upgrade.msix')
        $script:package = Get-AppxPackage -Name MeowcalSub.StoreCITest
        if ($script:package.Version -ne '1.0.1.0' -or (Get-FileHash $sentinel).Hash -ne $before) { throw 'Upgrade version or data retention failed.' }
        foreach ($config in $configBefore) {
            if ((Get-FileHash -LiteralPath $config.Path).Hash -ne $config.Hash) { throw 'Upgrade changed application configuration.' }
        }
        @{package=$script:package.PackageFullName;sentinelRetained=$true;retainedConfigurationFiles=$configBefore.Count;versionOnlyUpgrade=$true}
    }
    $null = Step 'upgraded-release-restart-and-translation' {
        $script:appPid = [Activation]::Launch($script:package.PackageFamilyName+'!App')
        $deadline = (Get-Date).AddSeconds(40)
        do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
        $sample = Invoke-AppScript "(async()=>{await window.__TAURI__.core.invoke('wizard_start_service');return await window.__TAURI__.core.invoke('wizard_test_translation',{sourceText:'Good morning.',sourceLanguage:'en-US',targetLanguage:'zh-CN'})})()"
        if (-not $sample.translatedText -or $sample.translatedText -eq 'Good morning.') { throw 'Upgraded application did not translate.' }
        $sample
    }
    Stop-TestApp
    $null = Step 'uninstall-removes-package-and-private-cache' {
        Remove-AppxPackage -Package $script:package.PackageFullName
        Start-Sleep 3
        if ((Get-AppxPackage -Name MeowcalSub.StoreCITest) -or (Test-Path $packageData)) { throw 'Package or private data remains after uninstall.' }
        @{registrationRemoved=$true;privateCacheRemoved=$true}
    }
    $result.complete = $true
} catch { $result.fatalError = $_.Exception.Message }
finally {
    Stop-TestApp
    $result.finished = (Get-Date).ToString('o')
    $result.verdict = if ($result.fatalError -or @($result.steps | Where-Object {$_.status -eq 'fail'}).Count) { 'partial-or-failed' } else { 'pass' }
    Save-Result
    if ($script:thumbprint) { Remove-Item -LiteralPath "Cert:\LocalMachine\TrustedPeople\$script:thumbprint" -ErrorAction Continue }
    Stop-Transcript
}






if ($result.verdict -ne 'pass') { exit 1 }
