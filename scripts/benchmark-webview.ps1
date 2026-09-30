. (Join-Path $PSScriptRoot 'tests/store-webview.ps1')

function Assert-BenchmarkEndpoint($App) {
    if ($App.HasExited) { throw 'The validated benchmark application exited.' }
    $listeners = @(Get-NetTCPConnection -LocalPort 9241 -State Listen -ErrorAction Stop |
        Select-Object -ExpandProperty OwningProcess -Unique)
    if ($listeners.Count -ne 1) { throw 'Benchmark debugging endpoint ownership is ambiguous.' }
    $owner = Get-CimInstance Win32_Process -Filter "ProcessId=$($listeners[0])"
    $seen = @{}
    while ($owner -and $owner.ProcessId -ne $App.Id) {
        if ($seen.ContainsKey([int]$owner.ProcessId)) { throw 'Invalid debugging process ancestry.' }
        $seen[[int]$owner.ProcessId] = $true
        $parent = Get-CimInstance Win32_Process -Filter "ProcessId=$($owner.ParentProcessId)"
        if (-not $parent -or $parent.CreationDate -gt $owner.CreationDate) {
            throw 'Debugging process ancestry is unavailable or reused.'
        }
        $owner = $parent
    }
    if (-not $owner -or ($owner.CreationDate.ToUniversalTime() - $App.StartTime.ToUniversalTime()).Duration().Ticks -ge 10) {
        throw 'Debugging endpoint does not belong to the validated benchmark application.'
    }
}

function Connect-BenchmarkWebView($App) {
    Assert-BenchmarkEndpoint $App
    $allPages = Invoke-RestMethod 'http://127.0.0.1:9241/json/list' -TimeoutSec 5
    $pages = @($allPages | Where-Object { $_.url -eq 'http://tauri.localhost/' })
    if ($pages.Count -ne 1) { throw 'Expected exactly one benchmark application WebView.' }
    $endpoint = [uri]$pages[0].webSocketDebuggerUrl
    if ($endpoint.Scheme -ne 'ws' -or $endpoint.Host -ne '127.0.0.1' -or $endpoint.Port -ne 9241) {
        throw 'Unexpected benchmark WebView endpoint.'
    }
    Assert-BenchmarkEndpoint $App
    $script:socket = [Net.WebSockets.ClientWebSocket]::new()
    $null = $script:socket.ConnectAsync($endpoint, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    $script:benchmarkApp = $App
    if (-not (Invoke-BenchmarkScript "typeof window.__TAURI__?.core?.invoke === 'function'")) {
        throw 'Benchmark application bridge is not ready.'
    }
}

function Invoke-BenchmarkScript([string]$Expression) {
    Assert-BenchmarkEndpoint $script:benchmarkApp
    Invoke-AppScript $Expression
}

function Resume-BenchmarkOcr($Process) {
    $status = [OcrDelay]::NtResumeProcess($Process.Handle)
    if ($Process.HasExited) { throw 'The original OCR process exited during the benchmark.' }
    if ($status -ne 0) {
        # The retained handle identifies the original child even if its PID is reused.
        try { $Process.Kill() } catch { throw "OCR resume failed ($status); original child cleanup failed: $_" }
        if (-not $Process.WaitForExit(5000)) { throw "OCR resume failed ($status); original child cleanup timed out." }
        throw "OCR resume failed ($status); terminated the original validated OCR child. Benchmark invalid."
    }
}
