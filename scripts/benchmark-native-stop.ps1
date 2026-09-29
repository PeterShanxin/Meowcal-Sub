param([Parameter(Mandatory)][string]$OutputFile, [ValidateRange(1,100)][int]$Samples = 10)
$ErrorActionPreference = 'Stop'
# Requires an owned, warm development app and the selected native subtitle fixture.
. (Join-Path $PSScriptRoot 'tests/store-webview.ps1')
Connect-AppWebView
$expression = @'
(async () => {
    const b = window.TauriBridge, rows = [];
    if (await b.invoke("is_translation_running")) throw new Error("Stop the existing session before benchmarking");
    await b.invoke("start_translation");
    try {
        for (let sample = 0; sample < SAMPLE_COUNT; sample++) {
            await new Promise(resolve => setTimeout(resolve, 350));
            const started = performance.now();
            await b.invoke("stop_translation");
            const stoppedMs = performance.now() - started;
            const running = await b.invoke("is_translation_running");
            let error = null;
            try { await b.invoke("start_translation"); } catch (failure) { error = String(failure); }
            rows.push({ stoppedMs, running, error });
            if (error) break;
        }
        return rows;
    } finally { await b.invoke("stop_translation"); }
})()
'@
try {
    Invoke-AppScript ($expression.Replace('SAMPLE_COUNT', [string]$Samples)) |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputFile
} finally { $script:socket.Dispose() }
