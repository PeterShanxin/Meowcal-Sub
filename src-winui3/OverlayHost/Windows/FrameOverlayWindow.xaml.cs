using Microsoft.UI.Xaml;
using Microsoft.UI.Windowing;
using Microsoft.UI;
using OverlayHost.Helpers;
using OverlayHost.Models;
using System.Diagnostics;
using WinRT.Interop;
using Windows.Graphics;

namespace OverlayHost.Windows;

/// <summary>
/// Borderless, transparent, topmost overlay window covering the entire virtual screen.
/// Used to display capture region borders and overlay UI.
/// </summary>
public sealed partial class FrameOverlayWindow : Window
{
    private AppWindow? _appWindow;
    private Region? _currentRegion;
    private OverlaySettings? _currentSettings;

    public FrameOverlayWindow()
    {
        InitializeComponent();

        // Remove title bar (borderless window)
        ExtendsContentIntoTitleBar = true;

        // Get AppWindow for size/position control
        var hwnd = WindowHelper.GetHwnd(this);
        var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
        _appWindow = AppWindow.GetFromWindowId(windowId);

        // Position window to cover entire virtual screen
        var (x, y, width, height) = WindowHelper.GetVirtualScreenBounds();
        _appWindow.MoveAndResize(new RectInt32(x, y, width, height));

        Debug.WriteLine($"[FrameOverlayWindow] Created overlay covering virtual screen: ({x}, {y}, {width}x{height})");
    }

    /// <summary>
    /// Show the overlay window and apply transparent/topmost styles.
    /// </summary>
    public void Show()
    {
        Debug.WriteLine("[FrameOverlayWindow] Showing overlay");

        // Activate window (make visible)
        Activate();

        // Apply Win32 styles for transparent, click-through, topmost overlay
        WindowHelper.MakeTransparentOverlay(this);

        Debug.WriteLine("[FrameOverlayWindow] Overlay shown with transparent styles applied");
    }

    /// <summary>
    /// Hide the overlay window.
    /// </summary>
    public void Hide()
    {
        Debug.WriteLine("[FrameOverlayWindow] Hiding overlay");
        _appWindow?.Hide();
    }

    /// <summary>
    /// Set the capture region to display (stores region for later rendering).
    /// </summary>
    /// <param name="region">The region to display, or null to clear</param>
    public void SetRegion(Region? region)
    {
        _currentRegion = region;

        if (region != null)
        {
            Debug.WriteLine($"[FrameOverlayWindow] Region set: ({region.X}, {region.Y}, {region.Width}x{region.Height})");
        }
        else
        {
            Debug.WriteLine("[FrameOverlayWindow] Region cleared");
        }

        // TODO: Draw region border on OverlayCanvas
        // Will be implemented in next phase with border rendering
    }

    /// <summary>
    /// Update overlay appearance settings (stores settings for later rendering).
    /// </summary>
    /// <param name="settings">New overlay settings</param>
    public void UpdateSettings(OverlaySettings settings)
    {
        _currentSettings = settings;

        Debug.WriteLine($"[FrameOverlayWindow] Settings updated: " +
                       $"Font={settings.FontFamily} {settings.FontSize}pt, " +
                       $"Colors=({settings.TextColor}, {settings.BackgroundColor}), " +
                       $"Border={settings.BorderColor} {settings.BorderWidth}px");

        // TODO: Apply settings to overlay UI elements
        // Will be implemented in next phase with border rendering
    }
}
