param(
    [Parameter(Mandatory)][int]$AppPid,
    [Parameter(Mandatory)][string]$AppExecutable,
    [Parameter(Mandatory)][string]$OutputFile,
    [ValidateRange(1,100)][int]$Samples = 10
)
$ErrorActionPreference = 'Stop'
# Requires an owned, warm development app and the selected native subtitle fixture.
. (Join-Path $PSScriptRoot 'benchmark-webview.ps1')
$app = Get-Process -Id $AppPid
$expression = @'
(async () => {
    const b = window.TauriBridge, rows = [];
    if (await b.invoke("is_translation_running")) throw new Error("Stop the existing session before benchmarking");
    await b.invoke("start_translation");
    try {
        for (let sample = 0; sample < SAMPLE_COUNT; sample++) {
            await new Promise(resolve => setTimeout(resolve, 350));
            if (!await b.invoke("is_translation_running")) throw new Error(`Capture session exited before sample ${sample}`);
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
    $null = $app.Handle
    if ($app.Path -ne (Resolve-Path $AppExecutable).Path) { throw 'Unexpected benchmark application executable.' }
    Connect-BenchmarkWebView $app
    Invoke-BenchmarkScript ($expression.Replace('SAMPLE_COUNT', [string]$Samples)) |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputFile
} finally {
    if ($script:socket) { $script:socket.Dispose() }
    $app.Dispose()
}
