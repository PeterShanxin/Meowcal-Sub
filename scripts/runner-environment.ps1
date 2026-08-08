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

function Expand-EnvironmentTokens {
    <#
        Expands `%NAME%` references using a supplied environment table.

        The Machine and User scopes store `REG_EXPAND_SZ` values, so a PATH entry
        can come back as the literal text `%SystemRoot%\System32`. Windows expands
        those at logon; reading the registry directly does not. Handing an
        unexpanded value to a process leaves it unable to resolve tools, and
        `Test-Path` rejects it outright, so `Find-CommandInPath` would report a
        perfectly good toolchain as missing.

        Expansion is against the reconstructed table rather than this process, so
        the result describes the runner's environment and not the caller's. A
        token naming something the table does not define is left alone: replacing
        it with emptiness would silently shorten PATH.
    #>
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
        [Parameter(Mandatory)][hashtable]$Environment
    )

    # Two passes: a value may reference a variable that itself contains a token.
    # Deeper nesting than that does not occur in practice and looping forever on a
    # self-referential value would be worse than leaving it.
    $expanded = $Value
    foreach ($pass in 1..2) {
        $expanded = [regex]::Replace($expanded, '%([^%]+)%', {
                param($match)
                $name = $match.Groups[1].Value
                foreach ($key in $Environment.Keys) {
                    if ($key -ieq $name) { return [string]$Environment[$key] }
                }
                $match.Value
            })
    }
    $expanded
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

    # Windows creates a large set of variables per session rather than storing
    # them in either registry scope: LOCALAPPDATA, APPDATA, TEMP, ProgramData,
    # ComSpec, PATHEXT, the PROCESSOR_* family, and more. A process without them
    # is not a clean environment, it is a broken one.
    #
    # An earlier version of this carried a handful of names explicitly and
    # dropped everything else. CI caught it: jobs on a runner started that way
    # failed with "LOCALAPPDATA is not set", and a Rust test that resolves the
    # app's config directory failed as well. A local gate cannot find this,
    # because only a job running *under* such a runner ever sees the environment.
    #
    # So anything this process carries that the registry does not define is kept,
    # except the contaminating names and PATH. PATH is the variable a shell most
    # often rewrites for its children and is already composed from the registry
    # above, which is what stops a shell-local toolchain from winning.
    $contaminated = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@(Get-RunnerContaminatedVariableNames), [System.StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in ([System.Environment]::GetEnvironmentVariables()).GetEnumerator()) {
        $name = [string]$entry.Key
        if ($name -ieq "Path" -or $contaminated.Contains($name)) { continue }
        if (-not $environment.ContainsKey($name)) { $environment[$name] = $entry.Value }
    }

    # Registry scopes hand back REG_EXPAND_SZ values verbatim, so this is where
    # `%SystemRoot%\System32` becomes a path the runner and Test-Path can use.
    $expanded = @{}
    foreach ($key in $environment.Keys) {
        $expanded[$key] = Expand-EnvironmentTokens -Value ([string]$environment[$key]) -Environment $environment
    }
    $expanded
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
        Runs a command with the sanitized environment and reports its first
        non-empty stdout line *and* its exit code.

        The exit code is returned rather than discarded because a broken or
        managed shim can print a plausible version and then fail. Accepting the
        printed line alone would let the preflight approve a toolchain that
        cannot actually run, which is the whole thing this check exists to catch.

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
    $standardError = $process.StandardError.ReadToEnd()
    $process.WaitForExit()

    [pscustomobject]@{
        Output   = @($standardOutput -split "`r?`n" | Where-Object { $_.Trim() }) | Select-Object -First 1
        Error    = @($standardError -split "`r?`n" | Where-Object { $_.Trim() }) | Select-Object -First 1
        ExitCode = $process.ExitCode
    }
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
