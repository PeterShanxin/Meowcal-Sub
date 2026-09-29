param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'The capture fixture is restricted to disposable hosted runners.'
}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class FixtureDpi {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
}
'@
[FixtureDpi]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$form = New-Object Windows.Forms.Form
$form.Text = 'Meowcal Store capture test'
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.Location = New-Object Drawing.Point(40,80)
$form.ClientSize = New-Object Drawing.Size(700,140)
$form.BackColor = [Drawing.Color]::Black
$form.TopMost = $true
$label = New-Object Windows.Forms.Label
$label.Dock = 'Fill'
$label.Text = 'Good morning.'
$label.ForeColor = [Drawing.Color]::White
$label.TextAlign = 'MiddleCenter'
$label.Font = New-Object Drawing.Font('Arial',36,[Drawing.FontStyle]::Regular,[Drawing.GraphicsUnit]::Pixel)
$form.Controls.Add($label)
$timer = New-Object Windows.Forms.Timer
$timer.Interval = 250
$timer.Add_Tick({ if (Test-Path (Join-Path $OutputDirectory 'stop-fixture')) { $form.Close() } })
$form.Add_Shown({
    $form.BringToFront()
    $form.Activate()
    @{x=$form.Left;y=$form.Top;width=$form.Width;height=$form.Height;scaleFactor=1} |
        ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'capture-region.json')
    $timer.Start()
})
try { [Windows.Forms.Application]::Run($form) }
finally { $timer.Dispose(); $form.Dispose() }
