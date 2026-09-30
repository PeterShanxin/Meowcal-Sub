[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Desktop preparation is restricted to disposable GitHub-hosted runners.'
}
. (Join-Path $PSScriptRoot 'store-desktop-target.ps1')
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$policyPath = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\OOBE'
New-Item -Path $policyPath -Force | Out-Null
New-ItemProperty -Path $policyPath -Name DisablePrivacyExperience -PropertyType DWord -Value 1 -Force | Out-Null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class StoreDesktopWindow {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetPackageFamilyName(IntPtr process, ref uint length, StringBuilder name);
}
"@
function Get-ForegroundProcess {
    $foregroundId = [uint32]0
    [StoreDesktopWindow]::GetWindowThreadProcessId([StoreDesktopWindow]::GetForegroundWindow(),[ref]$foregroundId) | Out-Null
    if (-not $foregroundId) { return }
    $process = Get-Process -Id $foregroundId
    $length = [uint32]512
    $family = [Text.StringBuilder]::new(512)
    $packageResult = [StoreDesktopWindow]::GetPackageFamilyName($process.Handle,[ref]$length,$family)
    [pscustomobject]@{id=$process.Id;name=$process.ProcessName;path=$process.Path;family=$(if ($packageResult -eq 0) { $family.ToString() } else { '' })}
}
$result = [ordered]@{policyEnabled=$true;keysSent=0;foreground=@();complete=$false}
try {
    # Hosted ARM images can retain an already-open CloudExperienceHost screen.
    # https://github.com/actions/runner-images/issues/14069
    foreach ($key in @('{ENTER}','{ENTER}','{ESC}')) {
        $process = Get-ForegroundProcess
        if (-not $process) { break }
        $result.foreground += $process
        if (-not (Test-StoreDesktopInputTarget -Process $process -Key $key -WindowsDirectory $env:WINDIR)) { continue }
        if ((Get-ForegroundProcess).id -ne $process.id) { throw 'Desktop focus changed during preparation.' }
        [Windows.Forms.SendKeys]::SendWait($key)
        $result.keysSent++
        Start-Sleep -Seconds 3
    }
    $result.complete = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    $result | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDirectory 'result.json') -Encoding UTF8
}
