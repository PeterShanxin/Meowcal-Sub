param(
    [Parameter(Mandatory)][int]$AppPid,
    [Parameter(Mandatory)][int]$OcrPid,
    [Parameter(Mandatory)][string]$AppExecutable,
    [Parameter(Mandatory)][string]$FixtureDirectory,
    [Parameter(Mandatory)][string]$OutputFile
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'benchmark-webview.ps1')
$app = Get-Process -Id $AppPid
$ocr = Get-Process -Id $OcrPid
$null = $app.Handle
$null = $ocr.Handle
$suspended = $false
$overlaySocket = $null
$result = [ordered]@{valid=$false;appPid=$AppPid;originalOcrPid=$OcrPid}
try {
    $identity = Get-CimInstance Win32_Process -Filter "ProcessId=$OcrPid"
    if ($app.Path -ne (Resolve-Path $AppExecutable).Path -or $app.HasExited -or $ocr.HasExited -or
        $identity.ParentProcessId -ne $AppPid -or $identity.Name -ne 'meowcal-core.exe' -or
        $identity.CreationDate -lt $app.StartTime -or
        ($identity.CreationDate.ToUniversalTime()-$ocr.StartTime.ToUniversalTime()).Duration().Ticks -ge 10) {
        throw 'Invalid original app/OCR identity.'
    }
    if (Get-CimInstance Win32_Process -Filter "ParentProcessId=$OcrPid" | Where-Object Name -ne 'conhost.exe') {
        throw 'Refusing to delay inference.'
    }
    Connect-BenchmarkWebView $app
    $mainSocket = $script:socket
    Invoke-BenchmarkScript '(async()=>{const b=window.TauriBridge;if(await b.invoke("is_translation_running"))throw new Error("Existing capture session");window.delayedEvents=[];window.delayedUnlisten=await b.event.listen("translation-update",e=>window.delayedEvents.push(e.payload));await b.invoke("start_translation");})()' | Out-Null
    $overlayDeadline = (Get-Date).AddSeconds(5)
    do {
        Assert-BenchmarkEndpoint $app
        $allPages = Invoke-RestMethod 'http://127.0.0.1:9241/json/list'
        $pages = @($allPages | Where-Object url -eq 'http://tauri.localhost/overlay.html')
        if ($pages.Count -gt 0 -or (Get-Date) -gt $overlayDeadline) { break }
        Start-Sleep -Milliseconds 100
    } while ($true)
    if ($pages.Count -ne 1) { throw "Expected one overlay WebView; found $($pages.Count)." }
    $endpoint = [uri]$pages[0].webSocketDebuggerUrl
    if ($endpoint.Scheme -ne 'ws' -or $endpoint.Host -ne '127.0.0.1' -or $endpoint.Port -ne 9241) { throw 'Invalid overlay endpoint.' }
    Assert-BenchmarkEndpoint $app
    $overlaySocket = [Net.WebSockets.ClientWebSocket]::new()
    $null = $overlaySocket.ConnectAsync($endpoint,[Threading.CancellationToken]::None).GetAwaiter().GetResult()
    Add-Type @'
using System; using System.Runtime.InteropServices;
public static class OcrDelay {
 [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr handle);
 [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr handle);
}
'@
    Assert-BenchmarkEndpoint $app
    if ($ocr.HasExited -or [OcrDelay]::NtSuspendProcess($ocr.Handle) -ne 0) { throw 'Original OCR could not be delayed.' }
    $suspended = $true
    @{id='obsolete-delayed';text='Please close the door behind you.'} | ConvertTo-Json | Set-Content -LiteralPath "$FixtureDirectory/cue.json"
    Start-Sleep -Milliseconds 600
    if ($ocr.HasExited) { throw 'Original OCR exited before Stop.' }
    # Switch the physical fixture before immediate restart; pending OCR still owns the old frame.
    @{id='new-delayed';text='Good morning.'} | ConvertTo-Json | Set-Content -LiteralPath "$FixtureDirectory/cue.json"
    Start-Sleep -Milliseconds 100
    $stopExpression = @'
(async()=>{
 const b=window.TauriBridge;
 if(!await b.invoke("is_translation_running"))throw new Error("Capture exited before Stop");
 const t=performance.now();await b.invoke("stop_translation");const stopMs=performance.now()-t;
 if(stopMs>3000)throw new Error("Stop exceeded 3s acceptance budget");
 if(await b.invoke("is_translation_running"))throw new Error("Stop left capture running");
 const stopped=window.delayedEvents.filter(e=>e.displayState==="stopped").at(-1);
 if(!stopped)throw new Error("Stop lifecycle event missing");
 const restarted=performance.now();await b.invoke("start_translation");
 return {stopMs,restartMs:performance.now()-restarted,stoppedSessionId:stopped.sessionId};
})()
'@
    $result.transition = Invoke-BenchmarkScript $stopExpression
    if (-not $ocr.WaitForExit(5000)) { throw 'Cancelled original OCR transport was not reaped.' }
    $suspended = $false
    $result.originalOcrExitCode = $ocr.ExitCode
    $deadline = (Get-Date).AddSeconds(20)
    do {
        $events = @(Invoke-BenchmarkScript 'window.delayedEvents')
        $translated = @($events | Where-Object { $_.displayState -eq 'translated' -and $_.original -eq 'Good morning.' })
        if ($translated.Count) { break }
        if ((Get-Date) -gt $deadline) { throw 'Restart did not produce the new authored translation.' }
        Start-Sleep -Milliseconds 250
    } while ($true)
    # Observe an additional interval for late results, rather than accepting the first event alone.
    Start-Sleep -Milliseconds 1000
    $result.events = @(Invoke-BenchmarkScript 'window.delayedEvents')
    $marker = $result.transition.stoppedSessionId
    $after = @($result.events | Where-Object { $_.displayState -ne 'stopped' -and $_.original })
    if (-not $after.Count -or @($after | Where-Object { $_.sessionId -le $marker -or $_.original -ne 'Good morning.' }).Count) {
        throw 'Old-session output reached the restarted session.'
    }
    if ($translated[0].translated -notmatch '早上好|早安') { throw 'Unexpected new translation.' }
    $script:socket = $overlaySocket
    try {
        $result.overlay = Invoke-BenchmarkScript '(async()=>({visible:await window.__TAURI__.window.getCurrentWindow().isVisible(),text:document.getElementById("subtitle-text").textContent}))()'
    } finally { $script:socket = $mainSocket }
    if (-not $result.overlay.visible -or $result.overlay.text -notmatch '早上好|早安') { throw 'New translation was not displayed in the visible native overlay.' }
    $result.running = Invoke-BenchmarkScript 'window.TauriBridge.invoke("is_translation_running")'
    if (-not $result.running) { throw 'Restarted capture exited.' }
    $result.valid = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    try {
        if ($suspended -and -not $ocr.HasExited) { Resume-BenchmarkOcr $ocr }
    } catch { $result.valid=$false; $result.error=$_.Exception.Message; throw }
    finally {
        try { $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputFile }
        finally {
            if ($overlaySocket) { $overlaySocket.Dispose() }
            if ($script:socket) { $script:socket.Dispose() }
            $ocr.Dispose()
            $app.Dispose()
        }
    }
}
