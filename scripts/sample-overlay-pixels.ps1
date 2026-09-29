param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$X = 151, [int]$Y = 798, [int]$Width = 1448, [int]$Height = 43,
    [ValidateRange(1,120)][int]$Seconds = 48
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PixelSampleDpi {
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
}
'@
[PixelSampleDpi]::SetThreadDpiAwarenessContext([IntPtr](-4)) | Out-Null
$bitmap = [Drawing.Bitmap]::new($Width, $Height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$buffer = [byte[]]::new($Width * $Height * 4)
$rows = [Collections.Generic.List[object]]::new()
$clock = [Diagnostics.Stopwatch]::StartNew()
try {
    while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
        $started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $graphics.CopyFromScreen($X, $Y, 0, 0, $bitmap.Size)
        $captured = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $bits = $bitmap.LockBits([Drawing.Rectangle]::new(0,0,$Width,$Height),
            [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try { [Runtime.InteropServices.Marshal]::Copy($bits.Scan0, $buffer, 0, $buffer.Length) }
        finally { $bitmap.UnlockBits($bits) }
        $rows.Add(@{startedAt=$started;capturedAt=$captured;hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($buffer))})
        Start-Sleep -Milliseconds 40
    }
    @{rectangle=@{x=$X;y=$Y;width=$Width;height=$Height};rows=$rows} |
        ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutputDirectory 'pixels.json')
    $bitmap.Save((Join-Path $OutputDirectory 'last-overlay.png'))
} finally { $graphics.Dispose(); $bitmap.Dispose() }

