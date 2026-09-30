$ErrorActionPreference = 'Stop'
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'benchmark-webview.ps1')
Add-Type 'public static class OcrDelay { public static int Status; public static int NtResumeProcess(System.IntPtr handle) { return Status; } }'

function Assert-Failure([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Message*") { throw }
        return
    }
    throw "Expected failure: $Message"
}
function Get-NetTCPConnection { $script:listeners | ForEach-Object { [pscustomobject]@{ OwningProcess = $_ } } }
function Get-CimInstance($Class, $Filter) { $script:processes[[int]($Filter.Split('=')[1])] }
function Invoke-RestMethod { $script:pages }

$created = [datetime]'2026-09-30T12:00:00Z'
$app = [pscustomobject]@{ Id = 100; HasExited = $false; StartTime = $created }
$script:listeners = @(200)
$script:processes = @{
    100 = [pscustomobject]@{ProcessId=100;ParentProcessId=1;CreationDate=$created}
    200 = [pscustomobject]@{ProcessId=200;ParentProcessId=100;CreationDate=$created.AddSeconds(1)}
}
Assert-BenchmarkEndpoint $app
$script:listeners = @()
Assert-Failure { Assert-BenchmarkEndpoint $app } 'ambiguous'
$script:listeners = @(200,300)
Assert-Failure { Assert-BenchmarkEndpoint $app } 'ambiguous'
$script:listeners = @(300)
Assert-Failure { Assert-BenchmarkEndpoint $app } 'does not belong'
$script:listeners = @(200)
$script:processes[100].CreationDate = $created.AddSeconds(2)
Assert-Failure { Assert-BenchmarkEndpoint $app } 'reused'
$script:processes[100].CreationDate = $created.AddSeconds(-1)
Assert-Failure { Assert-BenchmarkEndpoint $app } 'does not belong'
$script:processes[100].CreationDate = $created
$script:processes[200].ParentProcessId = 200
Assert-Failure { Assert-BenchmarkEndpoint $app } 'Invalid'
$script:processes[200].ParentProcessId = 100
$app.HasExited = $true
Assert-Failure { Assert-BenchmarkEndpoint $app } 'exited'
$app.HasExited = $false
$script:pages = @(@{url='http://tauri.localhost/'}, @{url='http://tauri.localhost/'})
Assert-Failure { Connect-BenchmarkWebView $app } 'exactly one'
$script:pages = @(@{url='http://tauri.localhost/';webSocketDebuggerUrl='ws://127.0.0.1:9242/devtools/page/1'})
Assert-Failure { Connect-BenchmarkWebView $app } 'Unexpected'
$script:benchmarkApp = $app
$script:listeners = @(300)
Assert-Failure { Invoke-BenchmarkScript 'start_translation' } 'does not belong'

$child = [pscustomobject]@{Handle=[IntPtr]123;HasExited=$false;Killed=$false;Waited=$false;ExitCompletes=$true}
$child | Add-Member ScriptMethod Kill { $this.Killed = $true }
$child | Add-Member ScriptMethod WaitForExit { param($Timeout); $this.Waited = $true; return $this.ExitCompletes }
[OcrDelay]::Status = 0
Resume-BenchmarkOcr $child
if ($child.Killed) { throw 'Successful resume killed the child.' }
[OcrDelay]::Status = -1
Assert-Failure { Resume-BenchmarkOcr $child } 'terminated the original'
if (-not $child.Killed -or -not $child.Waited) { throw 'Resume failure did not recover the original handle.' }
$child.Killed = $false
$child.HasExited = $true
Assert-Failure { Resume-BenchmarkOcr $child } 'exited'
if ($child.Killed) { throw 'Exited original child triggered cleanup of a reused PID.' }
$child.HasExited = $false
$child.ExitCompletes = $false
Assert-Failure { Resume-BenchmarkOcr $child } 'cleanup timed out'

# Exercise the actual preflight without loading native suspension or opening a socket.
$benchmarkPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'benchmark-delayed-ocr.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($benchmarkPath, [ref]$tokens, [ref]$parseErrors)
$validation = $ast.Find({param($node) $node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.Contains('$ocr.ParentProcessId')}, $false)
$validateChild = [scriptblock]::Create($validation.Extent.Text)
$AppExecutable = $benchmarkPath
$AppPid = 100
$appProcess = [pscustomobject]@{HasExited=$false;StartTime=$created}
$process = [pscustomobject]@{HasExited=$false;StartTime=$created.AddSeconds(-1)}
$app = [pscustomobject]@{CreationDate=$created;ExecutablePath=$benchmarkPath}
$ocr = [pscustomobject]@{CreationDate=$process.StartTime;ParentProcessId=100;Name='meowcal-core.exe'}
Assert-Failure { & $validateChild } 'not a child'
$process.StartTime = $created.AddSeconds(1)
$ocr.CreationDate = $process.StartTime
& $validateChild
node --test (Join-Path $PSScriptRoot 'benchmark-stop.test.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Benchmark session regression tests failed.' }
Write-Host 'Benchmark endpoint and OCR cleanup regressions passed.'
