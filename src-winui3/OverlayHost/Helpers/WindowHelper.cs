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
    // Architecture-aware P/Invoke for GetWindowLong/SetWindowLong
    // On x64, must use GetWindowLongPtr/SetWindowLongPtr with IntPtr return type
    // On x86, use GetWindowLong/SetWindowLong with int return type

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtr", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "GetWindowLong", SetLastError = true)]
    private static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtr", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll", EntryPoint = "SetWindowLong", SetLastError = true)]
    private static extern int SetWindowLong32(IntPtr hWnd, int nIndex, int dwNewLong);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT lpPoint);

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }

    private const int SM_XVIRTUALSCREEN = 76;
    private const int SM_YVIRTUALSCREEN = 77;
    private const int SM_CXVIRTUALSCREEN = 78;
    private const int SM_CYVIRTUALSCREEN = 79;

    /// <summary>
    /// Architecture-aware wrapper for GetWindowLong/GetWindowLongPtr.
    /// </summary>
    private static IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex)
    {
        if (IntPtr.Size == 8) // 64-bit
            return GetWindowLongPtr64(hWnd, nIndex);
        else // 32-bit
            return new IntPtr(GetWindowLong32(hWnd, nIndex));
    }

    /// <summary>
    /// Architecture-aware wrapper for SetWindowLong/SetWindowLongPtr.
    /// </summary>
    private static IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong)
    {
        if (IntPtr.Size == 8) // 64-bit
            return SetWindowLongPtr64(hWnd, nIndex, dwNewLong);
        else // 32-bit
            return new IntPtr(SetWindowLong32(hWnd, nIndex, dwNewLong.ToInt32()));
    }
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
        exStyle = new IntPtr(exStyle.ToInt64() | (long)(WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW));

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
        exStyle = new IntPtr(exStyle.ToInt64() & ~(long)WS_EX_TRANSPARENT);

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
        exStyle = new IntPtr(exStyle.ToInt64() | (long)WS_EX_TRANSPARENT);

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

    /// <summary>
    /// Get the current cursor position in screen coordinates.
    /// </summary>
    public static global::Windows.Foundation.Point GetCursorPosition()
    {
        GetCursorPos(out POINT point);
        return new global::Windows.Foundation.Point(point.X, point.Y);
    }
}
