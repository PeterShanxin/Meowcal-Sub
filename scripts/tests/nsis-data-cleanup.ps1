[CmdletBinding()]
param(
    [string]$MakeNsis = (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'),
    [string]$Hooks = (Join-Path $PSScriptRoot '..\..\src-tauri\windows\hooks.nsh')
)

$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) "meowcal-nsis-$([guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $root
try {
    # Give the compiled hook an isolated data root, including its Core paths.
    $isolatedHooks = (Get-Content -LiteralPath $Hooks -Raw).Replace('$APPDATA', '$EXEDIR\roaming')
    $isolatedHooks.Replace('$LOCALAPPDATA', '$EXEDIR\local') |
        Set-Content -LiteralPath (Join-Path $root 'hooks.nsh') -Encoding utf8
    Set-Content -LiteralPath (Join-Path $root 'probe.nsi') -Encoding utf8 -Value @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!include LogicLib.nsh
!include FileFunc.nsh
!define BUNDLEID "com.meowcal.sub"
!include "hooks.nsh"
Var DeleteAppDataCheckboxState
Var UpdateMode
OutFile "probe.exe"

Section
  StrCpy $DeleteAppDataCheckboxState 1
  StrCpy $UpdateMode 0
  ${GetParameters} $R0
  ${GetOptions} $R0 "/KEEP" $R1
  ${IfNot} ${Errors}
    StrCpy $DeleteAppDataCheckboxState 0
  ${EndIf}
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  ; Match the stock Tauri uninstaller's first cleanup pass.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RmDir /r "$EXEDIR\roaming\${BUNDLEID}"
    RmDir /r "$EXEDIR\local\${BUNDLEID}"
  ${EndIf}
  !insertmacro NSIS_HOOK_POSTUNINSTALL
SectionEnd
'@
    & $MakeNsis /V2 (Join-Path $root 'probe.nsi')
    if ($LASTEXITCODE -ne 0) { throw "NSIS compilation failed ($LASTEXITCODE)." }

    foreach ($scenario in @('transient-roaming', 'transient-local', 'keep', 'update', 'locked')) {
        $fixture = Join-Path $root $scenario
        $roaming = Join-Path $fixture 'roaming\com.meowcal.sub'
        $local = Join-Path $fixture 'local\com.meowcal.sub'
        $null = New-Item -ItemType Directory -Path $roaming, $local
        Copy-Item (Join-Path $root 'probe.exe') $fixture
        foreach ($path in @($roaming, $local)) {
            Set-Content -LiteralPath (Join-Path $path 'config.json') -Value 'fixture'
            Set-Content -LiteralPath (Join-Path $path 'config.bak.json') -Value 'fixture'
        }
        $lockRoot = if ($scenario -eq 'transient-local') { $local } else { $roaming }
        $config = Join-Path $lockRoot 'config.json'
        $backup = Join-Path $lockRoot 'config.bak.json'
        $lock = $null
        try {
            if ($scenario -like 'transient-*' -or $scenario -eq 'locked') {
                $lock = [IO.File]::Open($config, 'Open', 'Read', 'Read')
            }
            $start = @{ FilePath = (Join-Path $fixture 'probe.exe'); PassThru = $true }
            if ($scenario -eq 'keep') { $start.ArgumentList = '/KEEP' }
            if ($scenario -eq 'update') { $start.ArgumentList = '/UPDATE' }
            $process = Start-Process @start
            if ($scenario -like 'transient-*') {
                $clock = [Diagnostics.Stopwatch]::StartNew()
                while ((Test-Path -LiteralPath $backup) -and $clock.ElapsedMilliseconds -lt 5000) {
                    Start-Sleep -Milliseconds 20
                }
                if ((Test-Path -LiteralPath $backup) -or -not (Test-Path -LiteralPath $config)) {
                    throw "The first cleanup pass did not leave only the locked file in $scenario."
                }
                Start-Sleep -Milliseconds 300
                $lock.Dispose()
                $lock = $null
            }
            if (-not $process.WaitForExit(5000)) {
                $process.Kill()
                throw "Cleanup did not finish within five seconds in $scenario."
            }
            if ($process.ExitCode -ne 0) { throw "NSIS failed ($($process.ExitCode)) in $scenario." }

            if ($scenario -in @('keep', 'update')) {
                foreach ($path in @($roaming, $local)) {
                    foreach ($name in @('config.json', 'config.bak.json')) {
                        if (-not (Test-Path -LiteralPath (Join-Path $path $name))) {
                            throw "Cleanup removed settings in $scenario."
                        }
                    }
                }
            } elseif ($scenario -eq 'locked') {
                if (-not (Test-Path -LiteralPath $config) -or (Test-Path -LiteralPath $local)) {
                    throw 'A persistent lock should leave only the locked app-data tree.'
                }
            } elseif ((Test-Path -LiteralPath $roaming) -or (Test-Path -LiteralPath $local)) {
                throw "Cleanup left app data after the lock released in $scenario."
            }
            Write-Output "PASS $scenario"
        } finally {
            if ($null -ne $lock) { $lock.Dispose() }
        }
    }
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
