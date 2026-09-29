param(
    [Parameter(Mandatory)][int]$AppPid,
    [Parameter(Mandatory)][int]$OcrPid,
    [Parameter(Mandatory)][string]$AppExecutable,
    [Parameter(Mandatory)][string]$OutputFile
)
$ErrorActionPreference = 'Stop'
$app = Get-CimInstance Win32_Process -Filter "ProcessId=$AppPid"
$ocr = Get-CimInstance Win32_Process -Filter "ProcessId=$OcrPid"
if ($app.ExecutablePath -ne (Resolve-Path $AppExecutable).Path -or
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
. (Join-Path $PSScriptRoot 'tests/store-webview.ps1')
Connect-AppWebView
$process = Get-Process -Id $OcrPid
try {
    Invoke-AppScript 'window.TauriBridge.invoke("start_translation")' | Out-Null
    if ([OcrDelay]::NtSuspendProcess($process.Handle) -ne 0) { throw 'Could not delay the owned OCR process.' }
    try {
        # Let one real capture reach OCR before stopping. The model stays warm.
        Start-Sleep -Milliseconds 350
        Invoke-AppScript '(async()=>{const b=window.TauriBridge,t=performance.now();await b.invoke("stop_translation");const stopMs=performance.now()-t;const running=await b.invoke("is_translation_running");let error=null;try{await b.invoke("start_translation")}catch(e){error=String(e)}return {stopMs,running,error};})()' |
            ConvertTo-Json -Depth 5 | Set-Content $OutputFile
    } finally { [OcrDelay]::NtResumeProcess($process.Handle) | Out-Null }
} finally { $process.Dispose() }
