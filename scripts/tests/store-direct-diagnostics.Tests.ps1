$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot 'store-direct-coexistence.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'Coexistence diagnostics did not parse.' }
# Extract only the diagnostic function: the hosted-only lifecycle is never invoked locally.
$definition = $ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Write-DirectDiagnostic'}, $false)
. ([scriptblock]::Create($definition.Extent.Text))
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('direct-diagnostics-' + [guid]::NewGuid().ToString('N'))
$output = Join-Path $testRoot 'output'
$script:directLogDirectory = Join-Path $testRoot 'private-logs'
$script:directConfig = Join-Path $testRoot 'config.json'
$script:directExecutable = Join-Path $testRoot 'fixture.exe'
$script:directProcess = $null
New-Item -ItemType Directory -Path $output,$script:directLogDirectory | Out-Null
try {
    'fixture' | Set-Content $script:directExecutable
    '{"sourceLanguage":"ja-JP","privateToken":"DO_NOT_PUBLISH_CONFIG"}' | Set-Content $script:directConfig
    @('Getting settings... DO_NOT_PUBLISH_LOG','Saving settings...','private OCR: DO_NOT_PUBLISH_OCR') |
        Set-Content (Join-Path $script:directLogDirectory 'startup.log')
    Write-DirectDiagnostic 'after-save-readback' ([pscustomobject]@{sourceLanguage='ja-JP';privateToken='DO_NOT_PUBLISH_RPC'})
    '{"sourceLanguage":"en-US","privateToken":"DO_NOT_PUBLISH_CONFIG"}' | Set-Content $script:directConfig
    Write-DirectDiagnostic 'after-direct-close'
    $raw = Get-Content -LiteralPath "$output/direct-coexistence.jsonl" -Raw
    if ($raw -match 'DO_NOT_PUBLISH') { throw 'Diagnostic output leaked unselected config, RPC or log content.' }
    $rows = @(Get-Content -LiteralPath "$output/direct-coexistence.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    if ($rows.Count -ne 2 -or $rows[0].settingsSourceLanguage -ne 'ja-JP' -or
        $rows[0].configSourceLanguage -ne 'ja-JP' -or $rows[1].configSourceLanguage -ne 'en-US' -or
        $rows[0].configHash -eq $rows[1].configHash -or $rows[0].startupEvents.Count -ne 2) {
        throw 'Diagnostics did not preserve stage-specific persistence/readback evidence.'
    }
    'invalid DO_NOT_PUBLISH_CONFIG' | Set-Content $script:directConfig
    Write-DirectDiagnostic 'invalid-config'
    $last = Get-Content -LiteralPath "$output/direct-coexistence.jsonl" -Tail 1 | ConvertFrom-Json
    if (-not $last.configParseFailed) { throw 'Invalid configuration was not recorded safely.' }
    if (Get-ChildItem $output -Filter '*.log') { throw 'Raw startup log entered the uploaded output directory.' }
} finally {
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedRoot.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
Write-Host 'Direct coexistence diagnostic privacy and persistence contracts passed.'
