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
function Stop-TestApp([string]$ExecutablePath) {
    if ($script:socket -and $script:socket.State -eq [Net.WebSockets.WebSocketState]::Open) {
        try { $null = Invoke-AppScript "window.__TAURI__.core.invoke('prepare_for_update')" } catch { $_.Exception.Message | Add-Content "$output/stop-errors.txt" }
        $script:socket.Dispose()
    }
    if ($script:appPid) {
        $process = Get-Process -Id $script:appPid -ErrorAction SilentlyContinue
        if (-not $ExecutablePath) {
            $ExecutablePath = if ($script:directExecutable -and $process.Path -eq $script:directExecutable) {
                $script:directExecutable
            } else { Join-Path $script:package.InstallLocation 'meowcal-sub.exe' }
        }
        if ($process -and $process.Path -eq $ExecutablePath) {
            $tree = @(Get-CimInstance Win32_Process)
            $ownedIds = [Collections.Generic.HashSet[uint32]]::new()
            $null = $ownedIds.Add([uint32]$script:appPid)
            do {
                $changed = $false
                foreach ($child in $tree) {
                    if ($ownedIds.Contains([uint32]$child.ParentProcessId) -and $ownedIds.Add([uint32]$child.ProcessId)) { $changed = $true }
                }
            } while ($changed)
            $owned = @(foreach ($ownedId in $ownedIds) {
                $child = Get-Process -Id $ownedId -ErrorAction SilentlyContinue
                if ($child) { $null = $child.Handle; $child }
            })
            $owned | Select-Object Id,ProcessName,Path,StartTime | ConvertTo-Json -Compress | Add-Content "$output/stopped-processes.jsonl"
            Stop-Process -Id $script:appPid
            $script:appPid = $null
            $deadline = (Get-Date).AddSeconds(20)
            do {
                $remaining = @($owned | Where-Object { -not $_.HasExited })
                if (-not $remaining.Count) { break }
                Start-Sleep -Milliseconds 200
            } while ((Get-Date) -lt $deadline)
            if ($remaining.Count) {
                $remaining | Select-Object Id,ProcessName,Path | ConvertTo-Json | Set-Content "$output/remaining-processes.json"
                throw 'Test application children remained alive after shutdown; deployment was not retried.'
            }
        }
        $script:appPid = $null
    }
}
. (Join-Path $PSScriptRoot 'store-direct-coexistence.ps1')
try {
    try { $result.defender = Get-MpComputerStatus | Select-Object AntivirusEnabled,RealTimeProtectionEnabled,BehaviorMonitorEnabled }
    catch { $result.defender = @{unavailable=$_.Exception.Message} }
    $result.webviewBefore = @(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*','HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue | Select-Object name,pv)
    $policy = 'HKLM:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
    New-Item $policy -Force | Out-Null
    New-ItemProperty $policy -Name '*' -Value '--remote-debugging-port=9241' -PropertyType String -Force | Out-Null
    $directReady = $false
    try {
        $null = Step 'direct-install-baseline-startup' { Install-DirectBaseline }
        $directReady = $true
    } catch {
        $_.Exception.Message | Set-Content "$output/direct-install-error.txt"
        Stop-TestApp -ExecutablePath $script:directExecutable
    }
    $null = Step 'trust-test-certificate-on-runner' {
        $certificate = Import-Certificate -FilePath (Join-Path $InputDirectory 'local-test.cer') -CertStoreLocation Cert:\LocalMachine\TrustedPeople
        $script:thumbprint = $certificate.Thumbprint
        $certificate.Thumbprint
    }
    $null = Step 'signed-install' {
        $framework = Get-AppxPackage Microsoft.VCLibs.140.00.UWPDesktop |
            Where-Object { $_.Architecture -eq $env:PROCESSOR_ARCHITECTURE -and [version]$_.Version -ge [version]'14.0.33728.0' }
        $result.frameworkBefore = @($framework | Select-Object PackageFullName,Version,Architecture)
        Save-Result
        $install = @{Path = (Join-Path $InputDirectory 'initial.msix')}
        if (-not $framework) { $install.DependencyPath = Join-Path $InputDirectory 'vclibs.appx' }
        Add-AppxPackage @install
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
            $script:appPid = [Activation]::Launch($script:package.PackageFamilyName+'!App')
            $deadline = (Get-Date).AddSeconds(40)
            do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
            $settings = Invoke-AppScript "(async()=>{const s=await window.__TAURI__.core.invoke('get_settings');if(s.sourceLanguage==='ja-JP')throw Error('Store inherited direct-install settings');s.translation.localEngine.cpuOnly=true;s.sourceLanguage='en-US';s.targetLanguage='zh-CN';await window.__TAURI__.core.invoke('save_settings',{settings:s});return s})()"
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
    try { $null = Step 'hosted-screen-capture-ocr-and-overlay' {
        $fixtureScript = Join-Path $PSScriptRoot 'store-capture-fixture.ps1'
        $fixture = Start-Process powershell.exe -WindowStyle Hidden -PassThru -ArgumentList @('-NoProfile', '-File', "`"$fixtureScript`"", '-OutputDirectory', "`"$output`"")
        try {
            $deadline = (Get-Date).AddSeconds(20)
            $regionPath = Join-Path $output 'capture-region.json'
            while (-not (Test-Path $regionPath)) {
                if ($fixture.HasExited -or (Get-Date) -gt $deadline) { throw 'Capture fixture did not become visible.' }
                Start-Sleep 1
            }
            $regionJson = Get-Content $regionPath -Raw
            $capture = Invoke-AppScript @"
(async()=>{
 const invoke=window.__TAURI__.core.invoke;
 const updates=[];
 const unlisten=await window.__TAURI__.event.listen('translation-update',e=>updates.push(e.payload));
 try {
   await invoke('set_capture_region',$regionJson);
   await invoke('start_translation');
   for(let i=0;i<30&&!updates.some(x=>x.displayState==='translated'&&/Good morning/i.test(x.original));i++) await new Promise(r=>setTimeout(r,1000));
   return {running:await invoke('is_translation_running'),updates};
 } finally {unlisten();}
})()
"@
            $capture | ConvertTo-Json -Depth 10 | Set-Content "$output/capture-events.json" -Encoding UTF8
            Add-Type -AssemblyName System.Windows.Forms,System.Drawing
            $bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
            $bitmap = New-Object Drawing.Bitmap($bounds.Width,$bounds.Height)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.CopyFromScreen($bounds.Location,[Drawing.Point]::Empty,$bounds.Size)
                $bitmap.Save("$output/capture-overlay.png",[Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $bitmap.Dispose() }
            $translated = @($capture.updates | Where-Object { $_.displayState -eq 'translated' -and $_.original -match 'Good morning' -and $_.translated -match '[\u4e00-\u9fff]' })
            if (-not $capture.running -or $translated.Count -eq 0) { throw 'No translated output from the real capture/OCR fixture.' }
            Connect-AppWebView 'http://tauri.localhost/overlay.html'
            $overlay = Invoke-AppScript "(async()=>({visible:await window.__TAURI__.window.getCurrentWindow().isVisible(),text:document.body.innerText}))()"
            if (-not $overlay.visible -or -not $overlay.text.Contains($translated[-1].translated)) { throw 'Native overlay did not display the translated text.' }
            @{capture=$capture;overlay=$overlay;environment='hosted Windows desktop, not physical hardware'}
        } finally {
            try {
                Connect-AppWebView
                $null = Invoke-AppScript "window.__TAURI__.core.invoke('stop_translation')"
            } finally {
                New-Item -ItemType File -Path "$output/stop-fixture" -Force | Out-Null
                if (-not $fixture.WaitForExit(5000)) { Stop-Process -Id $fixture.Id }
            }
        }
    } } catch { $_.Exception.Message | Set-Content "$output/capture-error.txt" }
    Stop-TestApp
    $null = Step 'custom-storage-restart-and-translation' {
        $configs = @(Get-ChildItem $packageData -Filter config.json -File -Recurse)
        if ($configs.Count -ne 1) { throw 'Expected one package-owned configuration.' }
        $script:customCore = Join-Path $env:RUNNER_TEMP ('store-external-core-' + [guid]::NewGuid().ToString('N'))
        Copy-Item -LiteralPath $physicalCore -Destination $script:customCore -Recurse
        $config = Get-Content -LiteralPath $configs[0].FullName -Raw | ConvertFrom-Json
        $engine = $config.translation.localEngine
        foreach ($field in @('executablePath', 'modelPath')) {
            $oldPath = $engine.managedRuntime.$field
            if (-not $oldPath.StartsWith($physicalCore.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
                throw "Managed $field is outside the package cache."
            }
            $engine.managedRuntime.$field = $script:customCore + $oldPath.Substring($physicalCore.Length)
        }
        $engine.engineCacheRoot = $script:customCore
        [IO.File]::WriteAllText($configs[0].FullName, ($config | ConvertTo-Json -Depth 15), [Text.UTF8Encoding]::new($false))
        $script:customModel = $engine.managedRuntime.modelPath
        $script:customHash = (Get-FileHash -LiteralPath $script:customModel).Hash
        $script:appPid = [Activation]::Launch($script:package.PackageFamilyName+'!App')
        $deadline = (Get-Date).AddSeconds(40)
        do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
        $settings = Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')"
        if ($settings.translation.localEngine.managedRuntime.modelPath -ne $script:customModel) {
            throw 'Restart did not use custom model storage.'
        }
        $sample = Invoke-AppScript "(async()=>{await window.__TAURI__.core.invoke('wizard_start_service');return await window.__TAURI__.core.invoke('wizard_test_translation',{sourceText:'Thank you.',sourceLanguage:'en-US',targetLanguage:'zh-CN'})})()"
        if (-not $sample.translatedText -or $sample.translatedText -eq 'Thank you.') { throw 'Custom storage did not translate.' }
        @{sample=$sample;modelPath=$script:customModel;modelHash=$script:customHash;configurationSeededByHarness=$true}
    }
    Stop-TestApp
    $null = Step 'windows-reset-clears-private-data-preserves-external-model' {
        $script:package | Reset-AppxPackage
        if (Test-Path $physicalCore) { throw 'Windows reset retained the private engine cache.' }
        if (Get-ChildItem $packageData -Filter config.json -File -Recurse -ErrorAction SilentlyContinue) {
            throw 'Windows reset retained application settings.'
        }
        if ((Get-FileHash -LiteralPath $script:customModel).Hash -ne $script:customHash) { throw 'Windows reset changed the external model.' }
        $script:appPid = [Activation]::Launch($script:package.PackageFamilyName+'!App')
        $deadline = (Get-Date).AddSeconds(40)
        do { try { Connect-AppWebView; break } catch { if ((Get-Date) -gt $deadline) { throw }; Start-Sleep 2 } } while ($true)
        $settings = Invoke-AppScript "window.__TAURI__.core.invoke('get_settings')"
        if ($settings.translation.localEngine.managedRuntime -or $settings.translation.localEngine.engineCacheRoot) {
            throw 'Reset application reused the external engine registration.'
        }
        @{privateCacheRemoved=$true;settingsReset=$true;externalModelRetained=$true;relaunchSucceeded=$true}
    }
    Stop-TestApp
    $null = Step 'uninstall-removes-package-and-private-cache' {
        Remove-AppxPackage -Package $script:package.PackageFullName
        Start-Sleep 3
        if ((Get-AppxPackage -Name MeowcalSub.StoreCITest) -or (Test-Path $packageData)) { throw 'Package or private data remains after uninstall.' }
        if ((Get-FileHash -LiteralPath $script:customModel).Hash -ne $script:customHash) { throw 'Uninstall changed the external model.' }
        @{registrationRemoved=$true;privateCacheRemoved=$true;externalModelRetained=$true}
    }
    if ($directReady) { $null = Step 'direct-install-survives-store-lifecycle' { Test-DirectBaselineAfterStoreRemoval } }
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
