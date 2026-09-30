function Test-StoreDesktopInputTarget {
    param($Process, [string]$Key, [string]$WindowsDirectory)
    if (-not $Process) { return $false }
    $privacyHost = $Process.name -eq 'WWAHost' -and
        $Process.path -eq (Join-Path $WindowsDirectory 'System32\WWAHost.exe') -and
        $Process.family -like 'Microsoft.Windows.CloudExperienceHost_*'
    $startMenu = $Key -eq '{ESC}' -and $Process.name -eq 'StartMenuExperienceHost' -and
        $Process.path.StartsWith((Join-Path $WindowsDirectory 'SystemApps\'),[StringComparison]::OrdinalIgnoreCase) -and
        $Process.family -like 'MicrosoftWindows.Client.CBS_*'
    return $privacyHost -or $startMenu
}
