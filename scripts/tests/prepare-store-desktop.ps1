[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Desktop preparation is restricted to disposable GitHub-hosted runners.'
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$policyPath = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\OOBE'
New-Item -Path $policyPath -Force | Out-Null
New-ItemProperty -Path $policyPath -Name DisablePrivacyExperience -PropertyType DWord -Value 1 -Force | Out-Null
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$header = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty, 'Choose privacy settings for your device')
function Get-PrivacyWindow {
    $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Children, [Windows.Automation.Condition]::TrueCondition)
    foreach ($window in $windows) {
        if ($window.FindFirst([Windows.Automation.TreeScope]::Descendants, $header)) { return $window }
    }
}
$result = [ordered]@{policyEnabled=$true;pagesCompleted=0;choicesDisabled=0;complete=$false}
try {
    # The policy prevents a future launch; an experience already open must finish.
    $deadline = (Get-Date).AddSeconds(40)
    $absentSince = $null
    while ($true) {
        $window = Get-PrivacyWindow
        if (-not $window) {
            if (-not $absentSince) { $absentSince = Get-Date }
            if (((Get-Date) - $absentSince).TotalSeconds -ge 2) { break }
            Start-Sleep -Milliseconds 200
            continue
        }
        $absentSince = $null
        if ((Get-Date) -gt $deadline -or $result.pagesCompleted -ge 8) { throw 'Privacy setup did not finish.' }
        $controls = $window.FindAll([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition)
        foreach ($control in $controls) {
            $toggle = $null
            if ($control.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern, [ref]$toggle)) {
                if ($toggle.Current.ToggleState -ne [Windows.Automation.ToggleState]::Off) {
                    for ($attempt = 0; $attempt -lt 2 -and $toggle.Current.ToggleState -ne [Windows.Automation.ToggleState]::Off; $attempt++) {
                        $toggle.Toggle()
                    }
                    if ($toggle.Current.ToggleState -ne [Windows.Automation.ToggleState]::Off) {
                        throw 'Could not turn off a runner privacy choice.'
                    }
                    $result.choicesDisabled++
                }
            }
        }
        $buttonNames = [Windows.Automation.OrCondition]::new(
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'Next'),
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'Accept'))
        $buttonCondition = [Windows.Automation.AndCondition]::new($buttonNames,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button))
        $button = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $buttonCondition)
        if (-not $button -or -not $button.Current.IsEnabled) {
            Start-Sleep -Milliseconds 200
            continue
        }
        $button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
        $result.pagesCompleted++
        Start-Sleep -Milliseconds 500
    }
    $result.complete = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    $result | ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'result.json') -Encoding UTF8
}
