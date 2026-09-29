param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LatencyDpi {
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@
[LatencyDpi]::SetThreadDpiAwarenessContext([IntPtr](-4)) | Out-Null
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$form = [Windows.Forms.Form]::new()
$form.Text = 'Subtitle latency fixture'
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.Location = [Drawing.Point]::new(100, 500)
$form.ClientSize = [Drawing.Size]::new(1200, 120)
$form.BackColor = [Drawing.Color]::Black
$form.TopMost = $true
$label = [Windows.Forms.Label]::new()
$label.Dock = 'Fill'
$label.ForeColor = [Drawing.Color]::White
$label.TextAlign = 'MiddleCenter'
$label.Font = [Drawing.Font]::new('Arial', 44, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
$form.Controls.Add($label)
$timer = [Windows.Forms.Timer]::new()
$timer.Interval = 20
$script:lastCue = ''
$timer.Add_Tick({
    if (Test-Path (Join-Path $OutputDirectory 'stop-fixture')) { $form.Close(); return }
    # Keep the authored input above desktop notifications without taking keyboard focus.
    [LatencyDpi]::SetWindowPos($form.Handle, [IntPtr](-1), 0, 0, 0, 0, 0x0013) | Out-Null
    $path = Join-Path $OutputDirectory 'cue.json'
    if (-not (Test-Path $path)) { return }
    try { $cue = Get-Content $path -Raw | ConvertFrom-Json } catch { return }
    if ($cue.id -eq $script:lastCue) { return }
    $script:lastCue = $cue.id
    $label.Text = $cue.text
    $label.Refresh()
    @{id=$cue.id; text=$cue.text; paintedAt=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()} |
        ConvertTo-Json -Compress | Add-Content (Join-Path $OutputDirectory 'cues.jsonl')
})
$form.Add_Shown({
    @{x=$form.Left;y=$form.Top;width=$form.Width;height=$form.Height;scaleFactor=1} |
        ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'capture-region.json')
    $timer.Start()
})
try { [Windows.Forms.Application]::Run($form) }
finally { $timer.Dispose(); $form.Dispose() }
