$script:cdpId = 0
function Connect-AppWebView([string]$PageUrl = 'http://tauri.localhost/') {
    $pages = Invoke-RestMethod 'http://127.0.0.1:9241/json/list' -TimeoutSec 5
    $page = $pages |
        Where-Object { $_.url -eq $PageUrl } | Select-Object -First 1
    if (-not $page) { throw 'Application WebView is not available.' }
    if ($script:socket) { $script:socket.Dispose() }
    $script:socket = [Net.WebSockets.ClientWebSocket]::new()
    $null = $script:socket.ConnectAsync([uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    if (-not (Invoke-AppScript "typeof window.__TAURI__?.core?.invoke === 'function'")) {
        throw 'Application WebView bridge is not ready.'
    }
}
function Invoke-AppScript([string]$Expression) {
    $script:cdpId++
    $request = @{id=$script:cdpId;method='Runtime.evaluate';params=@{expression=$Expression;awaitPromise=$true;returnByValue=$true}} | ConvertTo-Json -Depth 6 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($request)
    $timeout = [Threading.CancellationTokenSource]::new(45000)
    try {
        $null = $script:socket.SendAsync([ArraySegment[byte]]::new($bytes),[Net.WebSockets.WebSocketMessageType]::Text,$true,$timeout.Token).GetAwaiter().GetResult()
        while ($true) {
            $buffer = [byte[]]::new(65536)
            $stream = [IO.MemoryStream]::new()
            do {
                $received = $script:socket.ReceiveAsync([ArraySegment[byte]]::new($buffer),$timeout.Token).GetAwaiter().GetResult()
                if ($received.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) { throw 'Application WebView closed.' }
                $stream.Write($buffer,0,$received.Count)
            } until ($received.EndOfMessage)
            $message = [Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json
            $stream.Dispose()
            if ($message.id -ne $script:cdpId) { continue }
            if ($message.error -or $message.result.exceptionDetails) { throw ($message | ConvertTo-Json -Depth 8 -Compress) }
            return $message.result.result.value
        }
    } finally { $timeout.Dispose() }
}
