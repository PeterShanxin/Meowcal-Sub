$ErrorActionPreference = 'Stop'
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'benchmark-webview.ps1')
if (-not ('OcrDelay' -as [type])) {
    Add-Type 'public static class OcrDelay { public static int Status; public static int NtResumeProcess(System.IntPtr handle) { return Status; } }'
}
[OcrDelay]::Status = -1
$process = [pscustomobject]@{Handle=[IntPtr]123;HasExited=$true;ExitCode=1;Killed=$false}
$process | Add-Member ScriptMethod Kill { $this.Killed = $true }
try { Resume-BenchmarkOcr $process; throw 'Exited OCR was accepted.' } catch {
    if ($_.Exception.Message -notlike '*resume status=-1, exit code=1*') {
        throw 'Exited OCR failure discarded the native resume status or original exit code.'
    }
}
if ($process.Killed) { throw 'Exited OCR cleanup attempted to kill a replacement.' }
$source = Join-Path (Split-Path -Parent $PSScriptRoot) 'benchmark-delayed-ocr.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'Delayed benchmark did not parse.' }
$outer = $ast.Find({param($node) $node -is [Management.Automation.Language.TryStatementAst] -and $node.Body.Extent.Text.Contains('$process = Get-Process')}, $false)
$handler = [scriptblock]::Create($outer.CatchClauses[0].Body.Statements.Extent.Text -join "`n")
$OutputFile = Join-Path ([IO.Path]::GetTempPath()) ('delayed-ocr-' + [guid]::NewGuid().ToString('N') + '.json')
$AppPid = 100
$OcrPid = 200
$result = [pscustomobject]@{stopMs=200;running=$false;error=$null;privateToken='DO_NOT_PUBLISH'}
try {
    '{"valid":true,"stopMs":123}' | Set-Content -LiteralPath $OutputFile
    try { throw 'original resume failure' } catch {
        try { & $handler; throw 'Failure handler accepted the sample.' } catch {
            if ($_.Exception.Message -ne 'original resume failure') { throw }
        }
    }
    if ((Get-Content -LiteralPath $OutputFile -Raw | ConvertFrom-Json).valid -ne $false) {
        throw 'Failed rerun left prior accepted output at the nominal path.'
    }
    $raw = Get-Content -LiteralPath "$OutputFile.failure.json" -Raw
    $saved = $raw | ConvertFrom-Json
    if ($saved.valid -ne $false -or $saved.ocrExitCode -ne 1 -or
        $saved.attemptedMeasurement.stopMs -ne 200 -or $raw.Contains('DO_NOT_PUBLISH')) {
        throw 'Invalid sample diagnostics were incomplete or leaked unselected fields.'
    }
} finally { Remove-Item -LiteralPath $OutputFile,"$OutputFile.failure.json" -ErrorAction SilentlyContinue }
Write-Host 'Delayed OCR exit diagnostics passed.'
