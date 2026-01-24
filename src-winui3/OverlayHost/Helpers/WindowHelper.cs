using Microsoft.UI.Xaml;
using System;
using System.Runtime.InteropServices;
using WinRT.Interop;

namespace OverlayHost.Helpers;

/// <summary>
/// Helper class for Win32 window operations (transparency, topmost, virtual screen bounds).
/// </summary>
public static class WindowHelper
{
    #region Win32 Constants
    private const int GWL_EXSTYLE = -20;
    private const uint WS_EX_LAYERED = 0x00080000;
    private const uint WS_EX_TRANSPARENT = 0x00000020;
    private const uint WS_EX_TOPMOST = 0x00000008;
    private const uint WS_EX_TOOLWINDOW = 0x00000080;
    #endregion

    #region Win32 P/Invoke
    [DllImport("user32.dll")]
    private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll")]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    private const int SM_XVIRTUALSCREEN = 76;
    private const int SM_YVIRTUALSCREEN = 77;
    private const int SM_CXVIRTUALSCREEN = 78;
    private const int SM_CYVIRTUALSCREEN = 79;
    #endregion

    /// <summary>
    /// Get the native HWND for a WinUI Window.
    /// </summary>
    public static IntPtr GetHwnd(Window window)
    {
        return WindowNative.GetWindowHandle(window);
    }

    /// <summary>
    /// Make the window a transparent, click-through, topmost overlay.
    /// Sets WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW.
    /// </summary>
    public static void MakeTransparentOverlay(Window window)
    {
        IntPtr hwnd = GetHwnd(window);
        IntPtr exStyle = GetWindowLongPtr(hwnd, GWL_EXSTYLE);

        // Add overlay styles: layered (for transparency), transparent (click-through),
        // topmost (always on top), toolwindow (no taskbar button)
        exStyle = new IntPtr(exStyle.ToInt64() | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW);

        SetWindowLongPtr(hwnd, GWL_EXSTYLE, exStyle);
    }

    /// <summary>
    /// Remove WS_EX_TRANSPARENT to make window interactive (receive mouse/keyboard input).
    /// </summary>
    public static void MakeInteractive(Window window)
    {
        IntPtr hwnd = GetHwnd(window);
        IntPtr exStyle = GetWindowLongPtr(hwnd, GWL_EXSTYLE);

        // Remove transparent flag to allow interaction
        exStyle = new IntPtr(exStyle.ToInt64() & ~WS_EX_TRANSPARENT);

        SetWindowLongPtr(hwnd, GWL_EXSTYLE, exStyle);
    }

    /// <summary>
    /// Add WS_EX_TRANSPARENT to make window click-through (ignore input).
    /// </summary>
    public static void MakeClickThrough(Window window)
    {
        IntPtr hwnd = GetHwnd(window);
        IntPtr exStyle = GetWindowLongPtr(hwnd, GWL_EXSTYLE);

        // Add transparent flag to pass through clicks
        exStyle = new IntPtr(exStyle.ToInt64() | WS_EX_TRANSPARENT);

        SetWindowLongPtr(hwnd, GWL_EXSTYLE, exStyle);
    }

    /// <summary>
    /// Get the bounds of the virtual screen (covers all monitors).
    /// Returns (X, Y, Width, Height) in screen coordinates.
    /// </summary>
    public static (int X, int Y, int Width, int Height) GetVirtualScreenBounds()
    {
        int x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        int height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        return (x, y, width, height);
    }
}
