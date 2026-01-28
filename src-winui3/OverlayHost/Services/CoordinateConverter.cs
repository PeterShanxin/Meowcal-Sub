using System;
using System.Runtime.InteropServices;
using OverlayHost.Models;

namespace OverlayHost.Services;

/// <summary>
/// Converts coordinates between logical (DPI-independent) and physical (actual screen) pixels.
/// CRITICAL: Windows screen capture APIs use physical pixels, while WinUI uses logical pixels.
/// </summary>
public static class CoordinateConverter
{
    #region Win32 P/Invoke

    [DllImport("user32.dll")]
    private static extern IntPtr MonitorFromPoint(POINT pt, uint dwFlags);

    [DllImport("shcore.dll")]
    private static extern int GetDpiForMonitor(IntPtr hmonitor, MonitorDpiType dpiType, out uint dpiX, out uint dpiY);

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int x;
        public int y;
    }

    private enum MonitorDpiType
    {
        MDT_EFFECTIVE_DPI = 0,  // Effective DPI that incorporates user scaling
        MDT_ANGULAR_DPI = 1,    // DPI based on viewing distance
        MDT_RAW_DPI = 2         // Raw DPI from the monitor
    }

    private const uint MONITOR_DEFAULTTONEAREST = 2; // Use nearest monitor if point not in any monitor

    #endregion

    private const double DEFAULT_DPI = 96.0; // 100% scaling

    /// <summary>
    /// Get DPI scale factor for a point on screen.
    /// NOTE: Coordinates must be in PHYSICAL PIXELS (virtual screen space).
    /// </summary>
    /// <param name="physicalX">X coordinate in physical pixels</param>
    /// <param name="physicalY">Y coordinate in physical pixels</param>
    /// <returns>Scale factor (1.0 = 100%, 1.5 = 150%, 2.0 = 200%)</returns>
    public static double GetDpiScaleForPoint(int physicalX, int physicalY)
    {
        var point = new POINT { x = physicalX, y = physicalY };
        IntPtr hMonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);

        if (hMonitor == IntPtr.Zero)
        {
            return 1.0; // Fallback to 100% if detection fails
        }

        int result = GetDpiForMonitor(hMonitor, MonitorDpiType.MDT_EFFECTIVE_DPI, out uint dpiX, out uint dpiY);

        if (result != 0)
        {
            return 1.0; // Fallback to 100% if GetDpiForMonitor fails
        }

        // Use horizontal DPI for scale calculation
        return dpiX / DEFAULT_DPI;
    }

    /// <summary>
    /// Convert logical coordinates (WinUI) to physical pixels (screen capture).
    /// Uses the DPI scale of the monitor at the logical position.
    /// </summary>
    /// <param name="logicalX">X position in logical pixels</param>
    /// <param name="logicalY">Y position in logical pixels</param>
    /// <param name="logicalWidth">Width in logical pixels</param>
    /// <param name="logicalHeight">Height in logical pixels</param>
    /// <returns>Region in physical pixels</returns>
    public static Region LogicalToPhysical(int logicalX, int logicalY, int logicalWidth, int logicalHeight)
    {
        if (logicalWidth <= 0 || logicalHeight <= 0)
        {
            throw new ArgumentException($"Invalid dimensions: {logicalWidth}x{logicalHeight}. Width and height must be positive.");
        }

        // Estimate physical center point for DPI detection (rough conversion at 1.0 scale)
        // This is okay because we're just finding which monitor, not doing precise conversion
        int estimatedPhysicalX = logicalX + logicalWidth / 2;
        int estimatedPhysicalY = logicalY + logicalHeight / 2;
        double scale = GetDpiScaleForPoint(estimatedPhysicalX, estimatedPhysicalY);

        // Convert to physical pixels by multiplying by scale
        return new Region
        {
            X = (int)Math.Round(logicalX * scale),
            Y = (int)Math.Round(logicalY * scale),
            Width = (int)Math.Round(logicalWidth * scale),
            Height = (int)Math.Round(logicalHeight * scale),
            CoordSpace = "physical"
        };
    }

    /// <summary>
    /// Convert physical pixels (screen capture) to logical coordinates (WinUI).
    /// Uses the DPI scale of the monitor at the physical position.
    /// </summary>
    /// <param name="physicalRegion">Region in physical pixels</param>
    /// <returns>Tuple (X, Y, Width, Height) in logical pixels</returns>
    public static (int X, int Y, int Width, int Height) PhysicalToLogical(Region physicalRegion)
    {
        // Get DPI scale for the center point of the region (using physical coordinates)
        int centerX = physicalRegion.X + physicalRegion.Width / 2;
        int centerY = physicalRegion.Y + physicalRegion.Height / 2;
        double scale = GetDpiScaleForPoint(centerX, centerY);

        // Convert to logical pixels by dividing by scale
        return (
            X: (int)Math.Round(physicalRegion.X / scale),
            Y: (int)Math.Round(physicalRegion.Y / scale),
            Width: (int)Math.Round(physicalRegion.Width / scale),
            Height: (int)Math.Round(physicalRegion.Height / scale)
        );
    }
}
