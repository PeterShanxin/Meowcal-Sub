[CmdletBinding()]
param([string]$BaselineRef='31882a0', [string]$OutputPath)
$ErrorActionPreference='Stop'
$repositoryRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$baseline=(& git -C $repositoryRoot rev-parse $BaselineRef).Trim()
if($LASTEXITCODE -ne 0){throw 'Baseline ref is unavailable.'}
$candidate=(& git -C $repositoryRoot rev-parse HEAD).Trim()
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('meowcal-realtime-'+[guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $scratch)
try {
    foreach($file in @('ocr_stability.rs','ocr_recent_lines.rs','ocr/band_cue.rs','ocr/band_verdict.rs')) {
        $content=& git -C $repositoryRoot show ($baseline+':src-tauri/src/'+$file)
        if($LASTEXITCODE -ne 0){throw "Baseline lacks $file"}
        [IO.File]::WriteAllText((Join-Path $scratch ([IO.Path]::GetFileName($file))),($content -join "`n"),[Text.UTF8Encoding]::new($false))
    }
    $basePath=$scratch.Replace('\','/')
    $repoPath=$repositoryRoot.Replace('\','/')
    $driver=@"
#![allow(dead_code)]
#[path = "$repoPath/src-tauri/src/llm/text_utils.rs"] pub mod text_utils;
pub mod llm { pub use crate::text_utils; }
#[path = "$repoPath/src-tauri/src/ocr_corruption.rs"] pub mod ocr_corruption;
#[path = "$basePath/ocr_stability.rs"] pub mod ocr_stability;
#[path = "$basePath/ocr_recent_lines.rs"] pub mod ocr_recent_lines;
#[path = "$repoPath/src-tauri/src/pipeline_session.rs"] pub mod pipeline_session;
#[path = "$repoPath/src-tauri/src/pipeline_pacing.rs"] pub mod pipeline_pacing;
#[path = "$repoPath/src-tauri/src/subtitle_timing.rs"] pub mod subtitle_timing;
#[path = "$repoPath/src-tauri/src/subtitle_queue.rs"] pub mod subtitle_queue;
mod legacy_band {
    #[path = "$basePath/band_verdict.rs"] mod band_verdict;
    #[path = "$basePath/band_cue.rs"] mod band_cue;
    #[derive(Default)] pub struct Gate(band_cue::Cue);
    impl Gate { pub fn observe(&mut self,text:&str,at:u64)->bool { self.0.observe(text,at,2000); self.0.verdict().is_included() } }
}
mod current_band {
    #[path = "$repoPath/src-tauri/src/ocr/band_verdict.rs"] mod band_verdict;
    #[path = "$repoPath/src-tauri/src/ocr/band_cue.rs"] mod band_cue;
    #[derive(Default)] pub struct Gate(band_cue::Cue);
    impl Gate { pub fn observe(&mut self,text:&str,at:u64)->bool { self.0.observe(text,at,2000); self.0.verdict().is_included() } }
}
#[path = "$repoPath/evals/realtime-subtitles/replay.rs"] mod replay;
fn main() { replay::run(); }
"@
    $sourcePath=Join-Path $scratch 'main.rs'
    [IO.File]::WriteAllText($sourcePath,$driver,[Text.UTF8Encoding]::new($false))
    $binaryPath=Join-Path $scratch 'replay.exe'
    & rustc --edition=2021 -O $sourcePath -o $binaryPath
    if($LASTEXITCODE -ne 0){throw 'Replay compilation failed.'}
    $json=& $binaryPath
    $result=$json | ConvertFrom-Json
    $result | Add-Member -NotePropertyName baseline_commit -NotePropertyValue $baseline
    $result | Add-Member -NotePropertyName candidate_commit -NotePropertyValue $candidate
    $result | Add-Member -NotePropertyName candidate_dirty -NotePropertyValue ([bool](& git -C $repositoryRoot status --porcelain --untracked-files=no))
    $report=$result | ConvertTo-Json -Depth 6
    if($OutputPath){ $reportPath=[IO.Path]::GetFullPath($OutputPath); [void](New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($reportPath)) -Force); [IO.File]::WriteAllText($reportPath,$report,[Text.UTF8Encoding]::new($false)) }
    $report
    if(-not $result.passed){exit 1}
} finally {
    $tempRoot=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
    $resolved=[IO.Path]::GetFullPath($scratch)
    if(-not $resolved.StartsWith($tempRoot,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -notlike 'meowcal-realtime-*'){throw 'Unsafe replay scratch path.'}
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
