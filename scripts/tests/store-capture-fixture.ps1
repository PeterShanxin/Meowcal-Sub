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
    $deadline = (Get-Date).AddSeconds(15)
    do {
        $form.BringToFront()
        $form.Activate()
        $form.Refresh()
        [Windows.Forms.Application]::DoEvents()
        $bitmap = New-Object Drawing.Bitmap($form.Width,$form.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($form.Location,[Drawing.Point]::Empty,$form.Size)
            $visible = $true
            foreach ($point in @([Drawing.Point]::new(5,5), [Drawing.Point]::new($form.Width-6,5),
                [Drawing.Point]::new(5,$form.Height-6), [Drawing.Point]::new($form.Width-6,$form.Height-6))) {
                $pixel = $bitmap.GetPixel($point.X,$point.Y)
                if ($pixel.R -gt 8 -or $pixel.G -gt 8 -or $pixel.B -gt 8) { $visible = $false; break }
            }
            $bitmap.Save((Join-Path $OutputDirectory 'capture-fixture.png'),[Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        if (-not $visible) { Start-Sleep -Milliseconds 100 }
    } while (-not $visible -and (Get-Date) -lt $deadline)
    if (-not $visible) {
        $script:fixtureFailed = $true
        'Capture fixture is obscured on the physical desktop.' | Set-Content (Join-Path $OutputDirectory 'capture-fixture-error.txt')
        $form.Close()
        return
    }
    @{x=$form.Left;y=$form.Top;width=$form.Width;height=$form.Height;scaleFactor=1} |
        ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'capture-region.json')
    $timer.Start()
})
$script:fixtureFailed = $false
try { [Windows.Forms.Application]::Run($form) }
finally { $timer.Dispose(); $form.Dispose() }

if ($script:fixtureFailed) { throw 'Capture fixture is obscured on the physical desktop.' }
