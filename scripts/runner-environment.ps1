# The clean-environment contract for the on-demand self-hosted runner.
#
# The runner is started in the foreground from whatever shell the operator is
# using, and a child process inherits that shell's environment. When the shell
# belongs to an IDE or a development agent, that agent's process environment
# silently becomes CI configuration.
#
# This is not hypothetical. During PR #80 verification a runner started from an
# agent session carried `NODE_OPTIONS=--require=...genie-safe-delete.cjs` and a
# managed Node 22/npm 10 ahead of the host's Node 24/npm 11. Lint and Tests
# passed because they never run `npm ci`; Frontend failed when the injected
# preload blocked npm from deleting files in `node_modules/.bin`. The repository
# was fine. The launch environment was not.
#
# The rule here is narrow on purpose: rebuild the environment a fresh logon would
# have, and drop a short, named list of preload variables that must never reach
# CI. Erasing arbitrary user variables would trade one unpredictable environment
# for another.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Variables removed from the runner environment no matter which scope set them.
#
# Both are Node preload/resolution hooks: they change what every `node` and `npm`
# process does before any repository code runs, which is precisely the failure
# above. CI never needs either, so dropping them costs nothing and removes a
# whole class of contamination. Keep this list short and justified - it is a
# contract, not a cleanup dumping ground.
$script:RunnerContaminatedVariables = @(
    "NODE_OPTIONS",
    "NODE_PATH"
)

function Get-RunnerContaminatedVariableNames {
    $script:RunnerContaminatedVariables
}

function Get-SanitizedRunnerEnvironment {
    <#
        Returns a new environment table with the contaminating names removed.
        The input is never mutated: the caller still needs its own environment
        intact after launching the runner.

        Names are compared case-insensitively because Windows environment
        variables are, and an agent setting `Node_Options` would otherwise slip
        through a case-sensitive match.
    #>
    param(
        [Parameter(Mandatory)][hashtable]$BaseEnvironment,
        [string[]]$ContaminatedNames = (Get-RunnerContaminatedVariableNames)
    )

    $removed = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@($ContaminatedNames), [System.StringComparer]::OrdinalIgnoreCase)

    $clean = @{}
    foreach ($key in $BaseEnvironment.Keys) {
        if (-not $removed.Contains($key)) {
            $clean[$key] = $BaseEnvironment[$key]
        }
    }
    $clean
}

function Get-CurrentProcessEnvironment {
    <#
        This process's environment as a plain table - the thing a runner would
        inherit if it were launched directly from here. Used to report what is
        being dropped, never to configure the runner.
    #>
    $environment = @{}
    foreach ($entry in ([System.Environment]::GetEnvironmentVariables()).GetEnumerator()) {
        $environment[$entry.Key] = $entry.Value
    }
    $environment
}

function Get-LogonEnvironment {
    <#
        Reads the Machine and User environment scopes rather than copying this
        process's environment. That is the difference that matters: a shell can
        prepend its own toolchain to PATH for its children, and copying the
        process environment would carry that injection into the runner. The
        registry scopes are what a freshly opened shell would see.

        PATH is the one variable that must be recombined rather than overwritten,
        because Windows composes it as machine-then-user at logon.
    #>
    $machine = [System.Environment]::GetEnvironmentVariables([System.EnvironmentVariableTarget]::Machine)
    $user = [System.Environment]::GetEnvironmentVariables([System.EnvironmentVariableTarget]::User)

    $environment = @{}
    foreach ($entry in $machine.GetEnumerator()) { $environment[$entry.Key] = $entry.Value }
    foreach ($entry in $user.GetEnumerator()) { $environment[$entry.Key] = $entry.Value }

    $environment["Path"] = Join-EnvironmentPath -MachinePath $machine["Path"] -UserPath $user["Path"]

    # A process still needs these, and they are per-session rather than stored in
    # either scope, so they are carried over deliberately instead of inherited
    # wholesale.
    foreach ($name in @("SystemRoot", "SystemDrive", "COMPUTERNAME", "USERNAME", "USERPROFILE", "USERDOMAIN")) {
        $value = [System.Environment]::GetEnvironmentVariable($name)
        if ($value -and -not $environment.ContainsKey($name)) { $environment[$name] = $value }
    }

    $environment
}

function Join-EnvironmentPath {
    <#
        Machine entries come first, then user entries, skipping blanks and exact
        duplicates. Order decides which `node.exe` wins, so this is the function
        that determines whether the runner uses the host toolchain or something
        a shell put in front of it.
    #>
    param([string]$MachinePath, [string]$UserPath)

    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $ordered = @()

    foreach ($segment in @($MachinePath, $UserPath)) {
        if (-not $segment) { continue }
        foreach ($entry in $segment.Split(";")) {
            $trimmed = $entry.Trim()
            if (-not $trimmed) { continue }
            if ($seen.Add($trimmed)) { $ordered += $trimmed }
        }
    }

    $ordered -join ";"
}

function Find-CommandInPath {
    <#
        Resolves a command against a specific PATH string instead of the current
        process's PATH. The whole point of this module is to answer "what will
        the runner see", which `Get-Command` cannot answer because it looks at
        this process.
    #>
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Command,
        [string[]]$Extensions = @(".exe", ".cmd", ".bat")
    )

    foreach ($directory in $Path.Split(";")) {
        $trimmed = $directory.Trim()
        if (-not $trimmed) { continue }
        foreach ($extension in $Extensions) {
            $candidate = Join-Path $trimmed ($Command + $extension)
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        }
    }
    return $null
}

function Invoke-InRunnerEnvironment {
    <#
        Runs a command with the sanitized environment and returns its first
        non-empty stdout line.

        Probing a toolchain with this process's environment answers the wrong
        question and gets it wrong in the exact case that matters: a broken
        NODE_OPTIONS preload makes `npm -v` print nothing, which reads as "npm is
        the wrong version" when npm is fine and the shell is not.

        `npm` is a .cmd shim on Windows and CreateProcess cannot execute a batch
        file directly, so those go through `cmd.exe /c`.
    #>
    param(
        [Parameter(Mandatory)][hashtable]$Environment,
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$Arguments = @()
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    if ([System.IO.Path]::GetExtension($FilePath) -in @(".cmd", ".bat")) {
        $startInfo.FileName = "cmd.exe"
        $startInfo.ArgumentList.Add("/c")
        $startInfo.ArgumentList.Add($FilePath)
    } else {
        $startInfo.FileName = $FilePath
    }
    foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }

    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.EnvironmentVariables.Clear()
    foreach ($key in $Environment.Keys) {
        $startInfo.EnvironmentVariables[[string]$key] = [string]$Environment[$key]
    }

    $process = [System.Diagnostics.Process]::Start($startInfo)
    $standardOutput = $process.StandardOutput.ReadToEnd()
    [void]$process.StandardError.ReadToEnd()
    $process.WaitForExit()

    @($standardOutput -split "`r?`n" | Where-Object { $_.Trim() }) | Select-Object -First 1
}

function Test-RunnerEnvironmentIsClean {
    <#
        Reports which contaminating variables are present in an environment.
        Returned rather than thrown so a caller can show every problem at once
        instead of one per run.
    #>
    param(
        [Parameter(Mandatory)][hashtable]$Environment,
        [string[]]$ContaminatedNames = (Get-RunnerContaminatedVariableNames)
    )

    $present = @()
    foreach ($name in $ContaminatedNames) {
        foreach ($key in $Environment.Keys) {
            if ($key -ieq $name -and $Environment[$key]) {
                $present += [pscustomobject]@{ Name = $key; Value = $Environment[$key] }
            }
        }
    }
    , $present
}
