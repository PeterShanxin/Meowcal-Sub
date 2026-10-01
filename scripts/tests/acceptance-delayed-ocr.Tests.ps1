$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path (Split-Path -Parent $PSScriptRoot) 'acceptance-delayed-ocr.ps1'),[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Acceptance script parse failed.'}
$discovery=$ast.FindAll({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -in @('$allPages','$pages')},$false)
function Invoke-RestMethod { ,@([pscustomobject]@{url='http://tauri.localhost/'},[pscustomobject]@{url='http://tauri.localhost/overlay.html'}) }
foreach($statement in $discovery){. ([scriptblock]::Create($statement.Extent.Text))}
if($pages.Count -ne 1 -or $pages[0].url -ne 'http://tauri.localhost/overlay.html'){throw 'Overlay discovery failed to expand the CDP REST collection.'}
$guard=$ast.Find({param($n) $n -is [Management.Automation.Language.IfStatementAst] -and $n.Extent.Text.Contains('Old-session output reached')},$false)
$marker=2
$restartFunction=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-RestartEvents'},$false)
. ([scriptblock]::Create($restartFunction.Extent.Text))
$events=@([pscustomobject]@{sessionId=1;original='A legitimate previous line.';displayState='translated'},[pscustomobject]@{sessionId=2;displayState='stopped'},[pscustomobject]@{sessionId=3;original='Good morning.';displayState='translated'})
$after=@(Get-RestartEvents $events $marker)
if($after.Count -ne 1 -or $after[0].sessionId -ne 3){throw 'Pre-Stop output was incorrectly classified as stale restart output.'}
& ([scriptblock]::Create($guard.Extent.Text))
foreach($bad in @([pscustomobject]@{sessionId=1;original='Good morning.'},[pscustomobject]@{sessionId=1;displayState='quiet'},[pscustomobject]@{sessionId=3;original='Please close the door behind you.'})){
    $after=@(Get-RestartEvents @($events[0],$events[1],$bad) $marker)
    try{& ([scriptblock]::Create($guard.Extent.Text));throw 'Stale output accepted.'}catch{if($_.Exception.Message -ne 'Old-session output reached the restarted session.'){throw}}
}
try{Get-RestartEvents @($events[0],$events[2]) $marker;throw 'Missing boundary accepted.'}catch{if($_.Exception.Message -ne 'Stop event boundary missing.'){throw}}
Write-Host 'Overlay discovery and stale-session acceptance regressions passed.'
