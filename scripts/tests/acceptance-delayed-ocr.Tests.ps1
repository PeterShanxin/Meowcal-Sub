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
$after=@([pscustomobject]@{sessionId=3;original='Good morning.'})
& ([scriptblock]::Create($guard.Extent.Text))
foreach($bad in @([pscustomobject]@{sessionId=1;original='Good morning.'},[pscustomobject]@{sessionId=3;original='Please close the door behind you.'})){
    $after=@($bad)
    try{& ([scriptblock]::Create($guard.Extent.Text));throw 'Stale output accepted.'}catch{if($_.Exception.Message -ne 'Old-session output reached the restarted session.'){throw}}
}
Write-Host 'Overlay discovery and stale-session acceptance regressions passed.'
