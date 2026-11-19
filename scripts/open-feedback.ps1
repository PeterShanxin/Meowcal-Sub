#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateRange(1, 2147483647)]
    [int]$Issue,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'meowcal-sub/feedback')
)
$ErrorActionPreference = 'Stop'
$repository = 'PeterShanxin/Meowcal-Sub'
$workflow = 'open-feedback.yml'
$stageName = 'meowcal-feedback-' + [guid]::NewGuid().ToString('N')
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$stage = Join-Path $tempRoot $stageName
$destination = [IO.Path]::GetFullPath((Join-Path $OutputDirectory "feedback-$Issue.zip"))
if (Test-Path -LiteralPath $destination) { throw "A ZIP already exists at $destination" }
if (!$IsWindows) { throw 'Run this tool with PowerShell 7 on Windows.' }
$gh = (Get-Command gh -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$null = New-Item -ItemType Directory -Path $stage -ErrorAction Stop
try {
    # The official Windows amd64 CLI runs on Windows 11 ARM64 through x64 emulation.
    $archive = Join-Path $stage 'age.zip'
    $release = 'https://github.com/FiloSottile/age/releases/download/v1.3.1/age-v1.3.1-windows-amd64.zip'
    Invoke-WebRequest -Uri $release -OutFile $archive -TimeoutSec 60
    $expected = 'c56e8ce22f7e80cb85ad946cc82d198767b056366201d3e1a2b93d865be38154'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ne $expected) {
        throw 'The age release checksum does not match.'
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $stage
    $age = Join-Path $stage 'age/age.exe'
    $keygen = Join-Path $stage 'age/age-keygen.exe'
    $identity = (& $keygen 2>$null) -join [Environment]::NewLine
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the temporary identity.' }
    $recipient = $identity | & $keygen -y 2>$null
    if ($LASTEXITCODE -ne 0 -or $recipient -notmatch '^age1[ac-hj-np-z02-9]{58}$') {
        throw 'Could not read the temporary recipient.'
    }

    $body = @{ ref = 'main'; inputs = @{ issue = "$Issue"; recipient = $recipient } } | ConvertTo-Json
    $run = $body | & $gh api --method POST "repos/$repository/actions/workflows/$workflow/dispatches" -H 'X-GitHub-Api-Version: 2026-03-10' --input - --jq .workflow_run_id
    if ($LASTEXITCODE -ne 0 -or $run -notmatch '^[1-9][0-9]*$') { throw 'Could not start the feedback workflow.' }
    Write-Host "Waiting for https://github.com/$repository/actions/runs/$run"
    & $gh run watch $run --repo $repository --exit-status --interval 5
    if ($LASTEXITCODE -ne 0) { throw 'The retrieval workflow did not succeed.' }

    $artifacts = Join-Path $stage 'artifacts'
    & $gh run download $run --repo $repository --name feedback-for-requester --dir $artifacts
    if ($LASTEXITCODE -ne 0) { throw 'Could not download the encrypted artifact.' }
    $null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination))
    $partial = Join-Path ([IO.Path]::GetDirectoryName($destination)) ($stageName + '.part')
    try {
        $identity | & $age --decrypt --identity - -o $partial (Join-Path $artifacts 'report.zip.age') 2>$null
        if ($LASTEXITCODE -ne 0) { throw 'The archive could not be decrypted.' }
        [IO.File]::Move($partial, $destination, $false)
    } finally {
        if ([IO.File]::Exists($partial)) { [IO.File]::Delete($partial) }
    }
    Write-Output $destination
} finally {
    $identity = $null
    $expectedStage = [IO.Path]::GetFullPath((Join-Path $tempRoot $stageName))
    $actual = (Resolve-Path -LiteralPath $stage).ProviderPath
    if ($actual -eq $expectedStage -and [IO.Path]::GetFileName($actual) -eq $stageName) {
        Remove-Item -LiteralPath $actual -Recurse -Force
    }
}
