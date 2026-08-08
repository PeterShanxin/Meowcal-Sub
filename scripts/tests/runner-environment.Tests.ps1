[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$scriptsDirectory = Split-Path -Parent $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scriptsDirectory
. (Join-Path $scriptsDirectory "runner-environment.ps1")

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Message)

    if (-not $Condition) {
        throw $Message
    }
}

# --- The measured failure ----------------------------------------------------

# This is PR #80's contamination exactly: an agent preload plus a managed Node
# resolution hook. Both must be gone from what the runner inherits.
$contaminated = @{
    "NODE_OPTIONS" = "--require=C:\agent\genie-safe-delete.cjs"
    "NODE_PATH"    = "C:\agent\managed\node_modules"
    "Path"         = "C:\Windows\system32"
    "USERPROFILE"  = "C:\Users\someone"
}

$clean = Get-SanitizedRunnerEnvironment -BaseEnvironment $contaminated

Assert-True (-not $clean.ContainsKey("NODE_OPTIONS")) "NODE_OPTIONS must not reach the runner."
Assert-True (-not $clean.ContainsKey("NODE_PATH")) "NODE_PATH must not reach the runner."

# Sanitizing must not become a general purge. Everything the runner legitimately
# needs has to survive, or the cure is worse than the contamination.
Assert-Equal "C:\Windows\system32" $clean["Path"] "PATH survives sanitization."
Assert-Equal "C:\Users\someone" $clean["USERPROFILE"] "Unrelated variables survive sanitization."

# The caller's own environment is still needed after the runner is launched.
Assert-True $contaminated.ContainsKey("NODE_OPTIONS") "Sanitizing does not mutate the input environment."

# Windows environment variables are case-insensitive, so a differently-cased
# injection is the same injection.
$mixedCase = Get-SanitizedRunnerEnvironment -BaseEnvironment @{ "Node_Options" = "--require=x" }
Assert-True (-not $mixedCase.ContainsKey("Node_Options")) "Case-insensitive contamination is removed."

# --- Detection ---------------------------------------------------------------

$detected = Test-RunnerEnvironmentIsClean -Environment $contaminated
Assert-Equal 2 $detected.Count "Both contaminating variables are reported."

Assert-Equal 0 (Test-RunnerEnvironmentIsClean -Environment $clean).Count `
    "A sanitized environment reports nothing."

# An empty value is not contamination; reporting it would train the operator to
# ignore the diagnostic.
Assert-Equal 0 (Test-RunnerEnvironmentIsClean -Environment @{ "NODE_OPTIONS" = "" }).Count `
    "An empty variable is not treated as contamination."

# --- PATH composition --------------------------------------------------------

# Machine entries come first, exactly as a logon shell composes them. This
# ordering is what decides which node.exe the runner executes.
Assert-Equal "C:\machine;C:\user" `
    (Join-EnvironmentPath -MachinePath "C:\machine" -UserPath "C:\user") `
    "Machine PATH entries precede user entries."

Assert-Equal "C:\machine;C:\user" `
    (Join-EnvironmentPath -MachinePath "C:\machine;;  ;C:\machine" -UserPath "C:\user;C:\MACHINE") `
    "Blank and duplicate PATH entries are dropped case-insensitively."

Assert-Equal "C:\user" (Join-EnvironmentPath -MachinePath $null -UserPath "C:\user") `
    "A missing machine PATH is tolerated."

# --- Resolving a command against a specific PATH -----------------------------

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-runner-env-$([System.Guid]::NewGuid().ToString('N'))"
try {
    $hostTools = Join-Path $fixtureRoot "host"
    $agentTools = Join-Path $fixtureRoot "agent"
    New-Item -ItemType Directory -Path $hostTools -Force | Out-Null
    New-Item -ItemType Directory -Path $agentTools -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $hostTools "node.exe") -Value "host" -Encoding ascii
    Set-Content -LiteralPath (Join-Path $agentTools "node.exe") -Value "agent" -Encoding ascii

    # The runner must resolve the host toolchain even when an agent directory
    # exists on the machine. Whichever comes first in PATH wins, which is why
    # PATH is rebuilt rather than inherited.
    Assert-Equal (Join-Path $hostTools "node.exe") `
        (Find-CommandInPath -Path "$hostTools;$agentTools" -Command "node") `
        "The first PATH entry wins."

    Assert-Equal (Join-Path $agentTools "node.exe") `
        (Find-CommandInPath -Path "$agentTools;$hostTools" -Command "node") `
        "PATH order is what selects the toolchain."

    Assert-True ($null -eq (Find-CommandInPath -Path $hostTools -Command "definitely-not-installed")) `
        "A missing command resolves to null rather than throwing."

    # npm ships as a .cmd shim on Windows, so extension probing has to cover it.
    Set-Content -LiteralPath (Join-Path $hostTools "npm.cmd") -Value "@echo off" -Encoding ascii
    Assert-Equal (Join-Path $hostTools "npm.cmd") `
        (Find-CommandInPath -Path $hostTools -Command "npm") `
        "A .cmd shim is resolved."
} finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Probing a toolchain uses the clean environment --------------------------

# The first version of this fix probed node/npm with the *caller's* environment.
# A broken NODE_OPTIONS preload made `npm -v` print nothing, and the runner
# refused to start blaming npm - which is the same wrong-environment mistake the
# whole contract exists to prevent. The probe must see only the clean values.
$probeEnvironment = Get-SanitizedRunnerEnvironment -BaseEnvironment @{
    "Path"        = $env:Path
    "SystemRoot"  = $env:SystemRoot
    "NODE_OPTIONS" = "--require=C:\definitely\missing\preload.cjs"
}

$node = Find-CommandInPath -Path $probeEnvironment["Path"] -Command "node"
if ($node) {
    $probe = Invoke-InRunnerEnvironment -Environment $probeEnvironment -FilePath $node -Arguments @("-v")
    Assert-True ($probe.Output -match '^v\d+') `
        "node -v probed in the clean environment returned '$($probe.Output)'; a dropped preload must not break it."
    Assert-Equal 0 $probe.ExitCode "A healthy node probe exits zero."
}

# npm is a .cmd shim. If executing it regresses, every npm version check
# silently fails.
$npm = Find-CommandInPath -Path $probeEnvironment["Path"] -Command "npm"
if ($npm) {
    $npmProbe = Invoke-InRunnerEnvironment -Environment $probeEnvironment -FilePath $npm -Arguments @("-v")
    Assert-True ($npmProbe.Output -match '^\d+') `
        "npm -v probed in the clean environment returned '$($npmProbe.Output)'; the .cmd shim must be executable."
    Assert-Equal 0 $npmProbe.ExitCode "A healthy npm probe exits zero."
}

# A shim can print something version-shaped and still fail. The exit code has to
# come back with the text, or the preflight approves a toolchain that cannot run.
$failingShim = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-failing-shim-$([System.Guid]::NewGuid().ToString('N')).cmd"
try {
    Set-Content -LiteralPath $failingShim -Value "@echo off`r`necho 24.0.0`r`nexit /b 3" -Encoding ascii
    $failed = Invoke-InRunnerEnvironment -Environment $probeEnvironment -FilePath $failingShim
    Assert-Equal "24.0.0" $failed.Output "The printed line is still reported."
    Assert-Equal 3 $failed.ExitCode "A nonzero exit is reported rather than discarded."
} finally {
    Remove-Item -LiteralPath $failingShim -Force -ErrorAction SilentlyContinue
}

# --- Registry values are expanded ---------------------------------------------

# Machine and User scopes store REG_EXPAND_SZ, so a PATH entry can arrive as the
# literal text `%SystemRoot%\System32`. Test-Path rejects that, so an unexpanded
# value makes Find-CommandInPath report an installed tool as missing.
$tokens = @{ "SystemRoot" = "C:\Windows"; "Path" = "%SystemRoot%\System32;%USERPROFILE%\.cargo\bin"; "USERPROFILE" = "C:\Users\someone" }
Assert-Equal "C:\Windows\System32;C:\Users\someone\.cargo\bin" `
    (Expand-EnvironmentTokens -Value $tokens["Path"] -Environment $tokens) `
    "Registry tokens are expanded against the reconstructed environment."

# An unknown token is left alone rather than replaced with emptiness, which would
# silently shorten PATH.
Assert-Equal "%NOT_A_VARIABLE%\bin" `
    (Expand-EnvironmentTokens -Value "%NOT_A_VARIABLE%\bin" -Environment $tokens) `
    "An unresolvable token is preserved."

Assert-True ((Get-RunnerBaseEnvironment)["Path"] -notmatch '%\w+%') `
    "The assembled logon PATH carries no unexpanded tokens."

# --- The environment the runner is actually launched with --------------------

# The whole failure class this section guards: an environment can be perfectly
# clean and still unusable. Rebuilding it from the registry scopes plus a
# hand-written list of session variables produced exactly that, and CI proved it
# three separate ways on one runner:
#
#   Playwright:  Chromium ... not found at undefined\Program Files\Google\Chrome\...
#   Rust:        the installed app's config resolved to bare "config.json"
#   #97 gate:    LOCALAPPDATA is not set
#
# Starting from the real process environment is complete by construction. These
# assertions pin that completeness so no future narrowing can quietly return.
$launched = Get-SanitizedRunnerEnvironment -BaseEnvironment (Get-RunnerBaseEnvironment)

Assert-True ($launched.ContainsKey("Path") -and $launched["Path"]) "The runner environment has a PATH."
Assert-True ($launched["Path"] -like "*;*") "The runner PATH has multiple entries."
Assert-True ($launched["Path"] -notmatch '%\w+%') "The runner PATH carries no unexpanded tokens."

# Each of these is load-bearing for a job that CI actually runs: Program Files
# for Playwright's browser lookup, LOCALAPPDATA/APPDATA for the app's config
# directory, TEMP/TMP for every toolchain that writes scratch files.
foreach ($required in @(
        "LOCALAPPDATA", "APPDATA", "TEMP", "TMP", "ProgramData",
        "ProgramFiles", "ProgramW6432", "SystemRoot", "SystemDrive",
        "USERPROFILE", "ComSpec", "PATHEXT", "NUMBER_OF_PROCESSORS")) {
    Assert-True ([bool]$launched[$required]) `
        "The runner environment must carry $required; a job cannot run without it."
}
Assert-True ([bool]$launched["ProgramFiles(x86)"]) `
    "The runner environment must carry ProgramFiles(x86)."

# TEMP and TMP have to name somewhere that exists, not merely be present.
foreach ($scratch in @("TEMP", "TMP")) {
    Assert-True (Test-Path -LiteralPath $launched[$scratch]) `
        "$scratch must point at a real directory, got '$($launched[$scratch])'."
}

# An ordinary variable belonging to the caller is none of this contract's
# business. Sanitizing removes what is known to break CI, not everything
# unfamiliar.
$env:MEOWCAL_UNRELATED_PROBE = "kept"
try {
    $withUnrelated = Get-SanitizedRunnerEnvironment -BaseEnvironment (Get-RunnerBaseEnvironment)
    Assert-Equal "kept" $withUnrelated["MEOWCAL_UNRELATED_PROBE"] `
        "An unrelated caller variable survives; narrow sanitization is the point."
} finally {
    Remove-Item Env:\MEOWCAL_UNRELATED_PROBE -ErrorAction SilentlyContinue
}

# ...but a caller-local PATH injection must not survive, because that is the
# shadowing that made CI resolve a managed Node over the host's.
$originalPath = $env:Path
try {
    $injected = Join-Path ([System.IO.Path]::GetTempPath()) "meowcal-injected-toolchain"
    $env:Path = "$injected;$originalPath"
    $afterInjection = Get-SanitizedRunnerEnvironment -BaseEnvironment (Get-RunnerBaseEnvironment)
    Assert-True ($afterInjection["Path"] -notlike "*$injected*") `
        "A PATH entry injected by the caller must not reach the runner."
} finally {
    $env:Path = $originalPath
}

# PATH comes from the registry composition rather than from this process.
$registryPath = Expand-EnvironmentTokens -Environment (Get-CurrentProcessEnvironment) -Value (
    Join-EnvironmentPath `
        -MachinePath ([System.Environment]::GetEnvironmentVariables([System.EnvironmentVariableTarget]::Machine))["Path"] `
        -UserPath ([System.Environment]::GetEnvironmentVariables([System.EnvironmentVariableTarget]::User))["Path"])
Assert-Equal $registryPath $launched["Path"] `
    "PATH is the machine-then-user composition, not whatever this process carries."

# Node and npm must resolve deterministically from that PATH.
foreach ($tool in @("node", "npm")) {
    Assert-True ([bool](Find-CommandInPath -Path $launched["Path"] -Command $tool)) `
        "$tool must resolve on the clean runner PATH."
}

Assert-Equal 0 (Test-RunnerEnvironmentIsClean -Environment $launched).Count `
    "The launched environment is clean."

# --- The documented start path does not bypass the contract ------------------

# Regression guard for the defect itself: the runner documentation used to tell
# operators to Start-Process run.cmd straight from their own shell, which is how
# an agent environment became CI configuration. The canonical start path has to
# go through the script that sanitizes.
$runnerDoc = Get-Content -LiteralPath (Join-Path $repositoryRoot "docs\SELF_HOSTED_RUNNERS.md") -Raw
Assert-True ($runnerDoc -match '-Mode\s+Start') `
    "docs/SELF_HOSTED_RUNNERS.md must document the sanitized start path."
Assert-True ($runnerDoc -notmatch 'Start-Process\s+-FilePath\s+\(Join-Path\s+\$runnerDirectory\s+"run\.cmd"\)') `
    "docs/SELF_HOSTED_RUNNERS.md must not tell operators to launch run.cmd from their own shell."

# One contract, not two. The agent guide and the post-install message used to send
# operators straight at run.cmd, which reproduces the very failure being fixed.
$agentGuide = Get-Content -LiteralPath (Join-Path $repositoryRoot "docs\AGENT_GUIDE.md") -Raw
Assert-True ($agentGuide -match '-Mode\s+Start') `
    "docs/AGENT_GUIDE.md must point at the sanitized start path."
Assert-True ($agentGuide -notmatch 'start the existing `run\.cmd`') `
    "docs/AGENT_GUIDE.md must not tell agents to start run.cmd directly."

$setupScript = Get-Content -LiteralPath (Join-Path $repositoryRoot "scripts\setup-self-hosted-runner.ps1") -Raw
Assert-True ($setupScript -notmatch 'Start it with: \$RunnerDirectory') `
    "The post-install message must not hand out the raw run.cmd path."

Write-Host "runner environment contract tests passed." -ForegroundColor Green
