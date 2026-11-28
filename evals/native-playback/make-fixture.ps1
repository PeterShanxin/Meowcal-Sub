[CmdletBinding()]
param(
    [ValidateSet('rapid-dialogue-30s', 'equal-width-120s', 'single-character-12s')]
    [string]$Scenario = 'rapid-dialogue-30s',
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$ffmpeg = (Get-Command ffmpeg -CommandType Application).Source
$manifest = Get-Content -LiteralPath "$PSScriptRoot/../band-gate/scenarios.json" -Raw -Encoding utf8 | ConvertFrom-Json
$selected = $manifest.scenarios | Where-Object id -CEQ $Scenario
$out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
if (Test-Path -LiteralPath $out) { throw 'OutputDirectory must not already exist.' }
$null = New-Item -ItemType Directory -Path $out

function Format-CueTime([double]$Seconds) {
    [TimeSpan]::FromMilliseconds([math]::Round(1000 * $Seconds)).ToString('hh\:mm\:ss\,fff')
}

$index = 0
$srt = foreach ($cue in $selected.segments) {
    if ($cue.expected -ne 'subtitle') { continue }
    $index += 1
    $index
    "$(Format-CueTime $cue.onset) --> $(Format-CueTime ($cue.onset + $cue.duration))"
    $cue.lines
    ''
}
$srt | Set-Content -LiteralPath "$out/cues.srt" -Encoding utf8
$selected | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath "$out/scenario.json" -Encoding utf8

Push-Location $out
try {
    $font = if ($Scenario -eq 'single-character-12s') { 'Microsoft YaHei' } else { 'Arial' }
    $style = "FontName=$font,Fontsize=24,Outline=2,Shadow=0,MarginV=22"
    & $ffmpeg -hide_banner -loglevel error -n -f lavfi -i 'testsrc2=s=1280x720:r=30' `
        -t $selected.durationSeconds -vf "boxblur=16:2,eq=brightness=-0.15:saturation=0.55,subtitles=cues.srt:force_style='$style'" `
        -an -c:v libx264 -preset fast -crf 21 -pix_fmt yuv420p -movflags +faststart 'fixture.mp4'
    if ($LASTEXITCODE -ne 0) { throw "FFmpeg failed ($LASTEXITCODE)." }
} finally { Pop-Location }
Write-Output "Created $Scenario clip and cue manifest in $out."
