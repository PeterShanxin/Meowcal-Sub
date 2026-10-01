param(
    [Parameter(Mandatory)][int]$AppPid,
    [Parameter(Mandatory)][int]$OcrPid,
    [Parameter(Mandatory)][string]$AppExecutable,
    [Parameter(Mandatory)][string]$OutputFile
)
$ErrorActionPreference = 'Stop'
$appProcess = $null
$process = $null
$result = $null
try {
    $appProcess = Get-Process -Id $AppPid
    $process = Get-Process -Id $OcrPid
    $null = $appProcess.Handle
    $null = $process.Handle
    $app = Get-CimInstance Win32_Process -Filter "ProcessId=$AppPid"
    $ocr = Get-CimInstance Win32_Process -Filter "ProcessId=$OcrPid"
    if ($appProcess.HasExited -or $process.HasExited -or
        ($app.CreationDate.ToUniversalTime() - $appProcess.StartTime.ToUniversalTime()).Duration().Ticks -ge 10 -or
        ($ocr.CreationDate.ToUniversalTime() - $process.StartTime.ToUniversalTime()).Duration().Ticks -ge 10 -or
        $ocr.CreationDate -lt $app.CreationDate -or
        $app.ExecutablePath -ne (Resolve-Path $AppExecutable).Path -or
        $ocr.ParentProcessId -ne $AppPid -or $ocr.Name -ne 'meowcal-core.exe') {
        throw 'The supplied OCR process is not a child of the owned test application.'
    }
    if (Get-CimInstance Win32_Process -Filter "ParentProcessId=$OcrPid" | Where-Object { $_.Name -ne 'conhost.exe' }) {
        throw 'Refusing to suspend a Core that owns an inference process.'
    }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OcrDelay {
    [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr process);
    [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr process);
}
'@
    . (Join-Path $PSScriptRoot 'benchmark-webview.ps1')
    Connect-BenchmarkWebView $appProcess
    Invoke-BenchmarkScript '(async()=>{if(await window.TauriBridge.invoke("is_translation_running"))throw new Error("Stop the existing session before benchmarking");await window.TauriBridge.invoke("start_translation")})()' | Out-Null
    Assert-BenchmarkEndpoint $appProcess
    if ([OcrDelay]::NtSuspendProcess($process.Handle) -ne 0) { throw 'Could not delay the owned OCR process.' }
    try {
        # Let one real capture reach OCR before stopping. The model stays warm.
        Start-Sleep -Milliseconds 350
        $result = Invoke-BenchmarkScript '(async()=>{const b=window.TauriBridge;if(!await b.invoke("is_translation_running"))throw new Error("Capture session exited before delayed OCR measurement");const t=performance.now();await b.invoke("stop_translation");const stopMs=performance.now()-t;const running=await b.invoke("is_translation_running");let error=null;try{await b.invoke("start_translation")}catch(e){error=String(e)}return {stopMs,running,error};})()'
    } finally { Resume-BenchmarkOcr $process }
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputFile
} catch {
    $failure = $_
    $evidence = [ordered]@{valid=$false;error=$failure.Exception.Message;appPid=$AppPid;ocrPid=$OcrPid}
    if ($result) {
        $evidence.attemptedMeasurement = @{stopMs=$result.stopMs;running=$result.running;restartError=$result.error}
    }
    if ($process) {
        $evidence.ocrExited = $process.HasExited
        if ($process.HasExited) { $evidence.ocrExitCode = $process.ExitCode }
    }
    try {
        $evidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputFile
        $evidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$OutputFile.failure.json"
    } catch { Write-Warning 'Could not persist the invalid delayed-OCR diagnostic.' }
    throw $failure
} finally {
    if ($script:socket) { $script:socket.Dispose() }
    if ($process) { $process.Dispose() }
    if ($appProcess) { $appProcess.Dispose() }
}
