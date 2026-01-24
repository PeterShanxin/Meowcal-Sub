using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Windowing;
using Microsoft.UI;
using System;
using System.Diagnostics;
using OverlayHost.Helpers;
using OverlayHost.Models;
using OverlayHost.Services;
using WinRT.Interop;
using Windows.Foundation;
using Windows.Graphics;

namespace OverlayHost.Windows;

/// <summary>
/// Fullscreen selector window for capture area selection.
/// Provides rubber-band selection with real-time dimension display.
/// </summary>
public sealed partial class SelectorWindow : Window
{
    // Selection state
    private bool _isSelecting = false;
    private bool _selectionComplete = false;
    private double _startX;
    private double _startY;
    private double _currentX;
    private double _currentY;

    // Window management
    private AppWindow? _appWindow;

    // Events
    public event EventHandler<Region>? SelectionConfirmed;
    public event EventHandler? SelectionCancelled;

    public SelectorWindow()
    {
        this.InitializeComponent();

        // Make window borderless
        this.ExtendsContentIntoTitleBar = true;

        // Get AppWindow for size/position control
        var hwnd = WindowHelper.GetHwnd(this);
        var windowId = Microsoft.UI.Win32Interop.GetWindowIdFromWindow(hwnd);
        _appWindow = AppWindow.GetFromWindowId(windowId);

        // Position window to cover entire virtual screen
        var (x, y, width, height) = WindowHelper.GetVirtualScreenBounds();
        _appWindow.MoveAndResize(new RectInt32(x, y, width, height));

        // Make window topmost but interactive (not click-through)
        WindowHelper.MakeInteractive(this);

        Debug.WriteLine($"[SelectorWindow] Initialized, covering virtual screen: ({x}, {y}) {width}x{height}");
    }

    /// <summary>
    /// Show the selector window and reset selection state.
    /// </summary>
    public void ShowSelector()
    {
        ResetSelection();
        this.Activate();
        Debug.WriteLine("[SelectorWindow] Selector activated");
    }

    /// <summary>
    /// Reset selection state and hide all UI elements.
    /// </summary>
    private void ResetSelection()
    {
        _isSelecting = false;
        _selectionComplete = false;
        SelectionBorder.Visibility = Visibility.Collapsed;
        DimensionReadout.Visibility = Visibility.Collapsed;
        ActionButtons.Visibility = Visibility.Collapsed;
        Debug.WriteLine("[SelectorWindow] Selection reset");
    }

    #region Pointer Event Handlers

    /// <summary>
    /// Start selection when pointer is pressed.
    /// </summary>
    private void OnPointerPressed(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(RootGrid);
        _startX = point.Position.X;
        _startY = point.Position.Y;
        _currentX = _startX;
        _currentY = _startY;
        _isSelecting = true;
        _selectionComplete = false;

        Debug.WriteLine($"[SelectorWindow] Selection started at ({_startX:F0}, {_startY:F0})");

        // Capture pointer for smooth dragging
        RootGrid.CapturePointer(e.Pointer);
    }

    /// <summary>
    /// Update selection rectangle as pointer moves.
    /// </summary>
    private void OnPointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_isSelecting)
            return;

        var point = e.GetCurrentPoint(RootGrid);
        _currentX = point.Position.X;
        _currentY = point.Position.Y;

        UpdateSelectionVisual();
    }

    /// <summary>
    /// Finalize selection when pointer is released.
    /// </summary>
    private void OnPointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_isSelecting)
            return;

        _isSelecting = false;
        _selectionComplete = true;

        // Release pointer capture
        RootGrid.ReleasePointerCaptures();

        // Show action buttons
        ActionButtons.Visibility = Visibility.Visible;

        var width = Math.Abs(_currentX - _startX);
        var height = Math.Abs(_currentY - _startY);
        Debug.WriteLine($"[SelectorWindow] Selection completed: {width:F0}x{height:F0}");
    }

    #endregion

    /// <summary>
    /// Update selection rectangle and dimension readout visuals.
    /// </summary>
    private void UpdateSelectionVisual()
    {
        // Calculate selection bounds
        double left = Math.Min(_startX, _currentX);
        double top = Math.Min(_startY, _currentY);
        double width = Math.Abs(_currentX - _startX);
        double height = Math.Abs(_currentY - _startY);

        // Update selection rectangle
        SelectionBorder.Margin = new Thickness(left, top, 0, 0);
        SelectionBorder.Width = width;
        SelectionBorder.Height = height;
        SelectionBorder.Visibility = Visibility.Visible;

        // Update dimension readout
        DimensionText.Text = $"{width:F0} × {height:F0}";
        DimensionReadout.Visibility = Visibility.Visible;

        // Position dimension readout near cursor (offset to avoid overlap)
        double readoutLeft = _currentX + 10;
        double readoutTop = _currentY + 10;

        // Keep readout within window bounds (with margin)
        if (readoutLeft + 100 > RootGrid.ActualWidth)
            readoutLeft = _currentX - 110;
        if (readoutTop + 30 > RootGrid.ActualHeight)
            readoutTop = _currentY - 40;

        DimensionReadout.Margin = new Thickness(readoutLeft, readoutTop, 0, 0);
    }

    /// <summary>
    /// Get the current selection region in logical coordinates (WinUI coordinates).
    /// Returns null if no selection is complete.
    /// </summary>
    private Rect? GetSelectionLogicalRegion()
    {
        if (!_selectionComplete)
            return null;

        double left = Math.Min(_startX, _currentX);
        double top = Math.Min(_startY, _currentY);
        double width = Math.Abs(_currentX - _startX);
        double height = Math.Abs(_currentY - _startY);

        return new Rect(left, top, width, height);
    }

    #region Button Event Handlers

    /// <summary>
    /// Confirm selection - convert to physical pixels and send to backend.
    /// </summary>
    private void ConfirmButton_Click(object sender, RoutedEventArgs e)
    {
        var logicalRegion = GetSelectionLogicalRegion();
        if (logicalRegion == null)
            return;

        // Extract coordinates from logical region
        int logicalX = (int)Math.Round(logicalRegion.Value.X);
        int logicalY = (int)Math.Round(logicalRegion.Value.Y);
        int logicalWidth = (int)Math.Round(logicalRegion.Value.Width);
        int logicalHeight = (int)Math.Round(logicalRegion.Value.Height);

        // Convert to physical pixels (screen capture coordinates)
        var physicalRegion = CoordinateConverter.LogicalToPhysical(
            logicalX, logicalY, logicalWidth, logicalHeight
        );

        Debug.WriteLine($"[SelectorWindow] Confirmed selection:");
        Debug.WriteLine($"  Logical: ({logicalX}, {logicalY}) {logicalWidth}x{logicalHeight}");
        Debug.WriteLine($"  Physical: ({physicalRegion.X}, {physicalRegion.Y}) {physicalRegion.Width}x{physicalRegion.Height}");

        // Raise event with physical coordinates
        SelectionConfirmed?.Invoke(this, physicalRegion);

        // Hide window
        _appWindow?.Hide();
    }

    /// <summary>
    /// Redraw - clear current selection and start over.
    /// </summary>
    private void RedrawButton_Click(object sender, RoutedEventArgs e)
    {
        Debug.WriteLine("[SelectorWindow] Redraw requested");
        ResetSelection();
    }

    /// <summary>
    /// Cancel selection - close without sending coordinates.
    /// </summary>
    private void CancelButton_Click(object sender, RoutedEventArgs e)
    {
        Debug.WriteLine("[SelectorWindow] Selection cancelled");
        SelectionCancelled?.Invoke(this, EventArgs.Empty);
        _appWindow?.Hide();
    }

    #endregion
}
