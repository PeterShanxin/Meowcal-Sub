using Microsoft.UI.Xaml;
using Microsoft.UI.Windowing;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml.Hosting;
using OverlayHost.Helpers;
using OverlayHost.Models;
using System.Diagnostics;
using System.Numerics;
using WinRT.Interop;
using Windows.Graphics;
using Windows.UI;

namespace OverlayHost.Windows;

/// <summary>
/// Borderless, transparent, topmost overlay window covering the entire virtual screen.
/// Used to display capture region borders and overlay UI.
/// </summary>
public sealed partial class FrameOverlayWindow : Window, IDisposable
{
    private AppWindow? _appWindow;
    private Region? _currentRegion;
    private OverlaySettings _settings = new(); // Use default settings if not yet synced
    private Compositor? _compositor;
    private SpriteVisual? _borderVisual;

    // Composition objects for border rendering (tracked for proper disposal)
    private CompositionRoundedRectangleGeometry? _rectangleGeometry;
    private CompositionSpriteShape? _rectangleShape;
    private ContainerVisual? _surfaceVisual;
    private CompositionVisualSurface? _visualSurface;
    private CompositionSurfaceBrush? _surfaceBrush;
    private DropShadow? _dropShadow;

    private bool _disposed = false;

    // Hover detection for selective click-through behavior
    private bool _isInteractive = false;
    private DispatcherTimer? _hoverCheckTimer;

    // Auto-fade behavior
    private DispatcherTimer? _autoFadeTimer;
    private bool _isFaded = false;
    private DateTime _lastInteractionTime = DateTime.Now;

    /// <summary>
    /// Event raised when user clicks settings button in overlay.
    /// </summary>
    public event EventHandler? SettingsRequested;

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

        // Initialize compositor for border rendering
        _compositor = ElementCompositionPreview.GetElementVisual(RootGrid).Compositor;

        // Setup hover detection for selective click-through
        SetupHoverDetection();

        // Setup auto-fade behavior
        SetupAutoFade();

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

        // Draw border ring around the region
        DrawBorderRing();
    }

    /// <summary>
    /// Update overlay appearance settings (stores settings for later rendering).
    /// </summary>
    /// <param name="settings">New overlay settings</param>
    public void UpdateSettings(OverlaySettings settings)
    {
        _settings = settings;

        Debug.WriteLine($"[FrameOverlayWindow] Settings updated: " +
                       $"Font={settings.FontFamily} {settings.FontSize}pt, " +
                       $"Colors=({settings.TextColor}, {settings.BackgroundColor}), " +
                       $"Border={settings.BorderColor} {settings.BorderWidth}px");

        // Apply font settings to subtitle text
        SubtitleText.FontSize = settings.FontSize;
        SubtitleText.FontFamily = new Microsoft.UI.Xaml.Media.FontFamily(settings.FontFamily);

        // Apply text color
        var textColor = ParseColor(settings.TextColor);
        SubtitleText.Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(textColor);

        // Note: Background color is set in XAML and cannot be easily changed at runtime with AcrylicBrush.
        // The default rgba(0,0,0,0.7) matches the hardcoded TintColor="Black" TintOpacity="0.7" in XAML.

        // Reposition subtitle panel if region is set
        if (_currentRegion != null)
        {
            PositionSubtitlePanel();
            DrawBorderRing();
        }
    }

    /// <summary>
    /// Update subtitle text and show the panel.
    /// </summary>
    /// <param name="original">Original OCR text (currently unused, reserved for future tooltip/debugging display)</param>
    /// <param name="translated">Translated subtitle text to display</param>
    /// <param name="backendUsed">Translation backend identifier (currently unused, reserved for future backend badge)</param>
    public void UpdateSubtitle(string original, string translated, string backendUsed)
    {
        _lastInteractionTime = DateTime.Now; // Reset fade timer on new subtitle

        Debug.WriteLine($"[FrameOverlayWindow] UpdateSubtitle: translated='{translated}', backend={backendUsed}");

        // Set subtitle text and show panel
        SubtitleText.Text = translated;
        SubtitlePanel.Visibility = Visibility.Visible;

        // Position panel below capture region
        PositionSubtitlePanel();
    }

    /// <summary>
    /// Clear subtitle text and hide the panel.
    /// </summary>
    public void ClearSubtitle()
    {
        Debug.WriteLine("[FrameOverlayWindow] ClearSubtitle");
        SubtitlePanel.Visibility = Visibility.Collapsed;
    }

    /// <summary>
    /// Position the subtitle panel below the capture region with configured offset.
    /// </summary>
    private void PositionSubtitlePanel()
    {
        if (_currentRegion == null)
        {
            Debug.WriteLine("[FrameOverlayWindow] Cannot position subtitle panel: no region set");
            return;
        }

        // Get virtual screen offset
        var (screenX, screenY, _, _) = WindowHelper.GetVirtualScreenBounds();

        // Calculate panel position (window-relative coordinates)
        var x = _currentRegion.X - screenX;
        var y = _currentRegion.Y - screenY + _currentRegion.Height + _settings.OffsetY;

        // Set position via margin (HorizontalAlignment=Left, VerticalAlignment=Top)
        SubtitlePanel.Margin = new Thickness(x, y, 0, 0);

        // Set max width (0 = match capture region width)
        SubtitlePanel.MaxWidth = _settings.MaxWidth > 0 ? _settings.MaxWidth : _currentRegion.Width;

        Debug.WriteLine($"[FrameOverlayWindow] Positioned subtitle panel: ({x}, {y}), maxWidth={SubtitlePanel.MaxWidth}");
    }

    /// <summary>
    /// Handle settings button click.
    /// Send message to backend to open settings window.
    /// </summary>
    private async void SettingsButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Debug.WriteLine("[FrameOverlayWindow] Settings button clicked");

            // Raise event to notify App to send IPC message
            SettingsRequested?.Invoke(this, EventArgs.Empty);

            await System.Threading.Tasks.Task.CompletedTask; // Placeholder for future async work
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[FrameOverlayWindow] Error in SettingsButton_Click: {ex.Message}");
        }
    }

    /// <summary>
    /// Draw border ring around the current region using Composition API.
    /// Creates a hollow rectangle with glow effect.
    /// </summary>
    private void DrawBorderRing()
    {
        // Dispose existing composition objects to prevent memory leaks
        _rectangleGeometry?.Dispose();
        _rectangleShape?.Dispose();
        _surfaceVisual?.Dispose();
        _visualSurface?.Dispose();
        _surfaceBrush?.Dispose();
        _dropShadow?.Dispose();
        _borderVisual?.Dispose();

        // Clear references
        _rectangleGeometry = null;
        _rectangleShape = null;
        _surfaceVisual = null;
        _visualSurface = null;
        _surfaceBrush = null;
        _dropShadow = null;
        _borderVisual = null;

        // Clear border if no region or compositor
        if (_currentRegion == null || _compositor == null)
        {
            ElementCompositionPreview.SetElementChildVisual(OverlayCanvas, null);
            Debug.WriteLine("[FrameOverlayWindow] Border cleared");
            return;
        }

        // Get border parameters
        var borderWidth = _settings.BorderWidth;
        var borderColor = ParseColor(_settings.BorderColor);

        // Convert region from physical (virtual screen) to window-relative coordinates
        var (screenX, screenY, _, _) = WindowHelper.GetVirtualScreenBounds();
        var x = (float)(_currentRegion.X - screenX);
        var y = (float)(_currentRegion.Y - screenY);
        var width = (float)_currentRegion.Width;
        var height = (float)_currentRegion.Height;

        Debug.WriteLine($"[FrameOverlayWindow] Drawing border: screen-relative ({x}, {y}, {width}x{height}), " +
                       $"borderWidth={borderWidth}, color={borderColor}");

        // Create shape visual for the border
        var shapeVisual = _compositor.CreateShapeVisual();
        shapeVisual.Size = new Vector2(width, height);

        // Create rectangle geometry (hollow border)
        var rectangleGeometry = _compositor.CreateRoundedRectangleGeometry();
        rectangleGeometry.Size = new Vector2(width, height);
        rectangleGeometry.CornerRadius = new Vector2(0); // Sharp corners

        // Create shape with stroke (no fill for hollow border)
        var rectangleShape = _compositor.CreateSpriteShape(rectangleGeometry);
        rectangleShape.StrokeThickness = borderWidth;
        rectangleShape.StrokeBrush = _compositor.CreateColorBrush(borderColor);

        shapeVisual.Shapes.Add(rectangleShape);

        // Create sprite visual container to hold the shape and apply shadow
        // SpriteVisual supports Shadow property, unlike ShapeVisual or ContainerVisual
        var spriteVisual = _compositor.CreateSpriteVisual();
        spriteVisual.Size = new Vector2(width, height);
        spriteVisual.Offset = new Vector3(x, y, 0);

        // Use CompositionVisualSurface to render the shape visual into a brush
        var surfaceVisual = _compositor.CreateContainerVisual();
        surfaceVisual.Size = new Vector2(width, height);
        surfaceVisual.Children.InsertAtTop(shapeVisual);

        // Create a visual surface from the shape
        var visualSurface = _compositor.CreateVisualSurface();
        visualSurface.SourceVisual = surfaceVisual;
        visualSurface.SourceSize = new Vector2(width, height);

        // Apply surface as brush to sprite visual
        var surfaceBrush = _compositor.CreateSurfaceBrush(visualSurface);
        spriteVisual.Brush = surfaceBrush;

        // Add drop shadow for glow effect
        var dropShadow = _compositor.CreateDropShadow();
        dropShadow.Color = borderColor;
        dropShadow.BlurRadius = 10f;
        dropShadow.Offset = new Vector3(0, 0, 0); // No offset - glow all around
        dropShadow.Opacity = 0.8f;

        spriteVisual.Shadow = dropShadow;

        // Store composition objects for proper disposal
        _rectangleGeometry = rectangleGeometry;
        _rectangleShape = rectangleShape;
        _surfaceVisual = surfaceVisual;
        _visualSurface = visualSurface;
        _surfaceBrush = surfaceBrush;
        _dropShadow = dropShadow;
        _borderVisual = spriteVisual;

        // Attach to overlay canvas
        ElementCompositionPreview.SetElementChildVisual(OverlayCanvas, _borderVisual);

        Debug.WriteLine($"[FrameOverlayWindow] Border rendered successfully");
    }

    /// <summary>
    /// Parse hex color string to Windows.UI.Color.
    /// Supports formats: #RGB, #RRGGBB, #AARRGGBB
    /// </summary>
    /// <param name="colorString">Hex color string (e.g., "#00A8FF")</param>
    /// <returns>Parsed color, or cyan if parsing fails</returns>
    private Color ParseColor(string colorString)
    {
        try
        {
            // Remove # prefix if present
            var hex = colorString.TrimStart('#');

            byte a = 255; // Default opaque
            byte r, g, b;

            switch (hex.Length)
            {
                case 3: // #RGB
                    r = Convert.ToByte(hex.Substring(0, 1) + hex.Substring(0, 1), 16);
                    g = Convert.ToByte(hex.Substring(1, 1) + hex.Substring(1, 1), 16);
                    b = Convert.ToByte(hex.Substring(2, 1) + hex.Substring(2, 1), 16);
                    break;

                case 6: // #RRGGBB
                    r = Convert.ToByte(hex.Substring(0, 2), 16);
                    g = Convert.ToByte(hex.Substring(2, 2), 16);
                    b = Convert.ToByte(hex.Substring(4, 2), 16);
                    break;

                case 8: // #AARRGGBB
                    a = Convert.ToByte(hex.Substring(0, 2), 16);
                    r = Convert.ToByte(hex.Substring(2, 2), 16);
                    g = Convert.ToByte(hex.Substring(4, 2), 16);
                    b = Convert.ToByte(hex.Substring(6, 2), 16);
                    break;

                default:
                    throw new FormatException($"Invalid hex color format: {colorString}");
            }

            return Color.FromArgb(a, r, g, b);
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[FrameOverlayWindow] Failed to parse color '{colorString}': {ex.Message}. Using default cyan.");
            return Color.FromArgb(255, 0, 168, 255); // Default to cyan
        }
    }

    /// <summary>
    /// Setup periodic mouse position checking for selective click-through behavior.
    /// </summary>
    private void SetupHoverDetection()
    {
        // Check mouse position periodically to toggle click-through
        _hoverCheckTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(100)
        };
        _hoverCheckTimer.Tick += CheckMouseHover;
        _hoverCheckTimer.Start();
    }

    /// <summary>
    /// Setup auto-fade timer to fade overlay to 30% opacity after inactivity.
    /// </summary>
    private void SetupAutoFade()
    {
        _autoFadeTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(500) // Check every 500ms
        };
        _autoFadeTimer.Tick += CheckAutoFade;
        _autoFadeTimer.Start();
    }

    /// <summary>
    /// Check if overlay should fade based on idle time.
    /// </summary>
    private void CheckAutoFade(object? sender, object e)
    {
        var idleTime = DateTime.Now - _lastInteractionTime;
        var timeoutMs = _settings.AutoFadeTimeoutMs;

        if (idleTime.TotalMilliseconds > timeoutMs && !_isFaded)
        {
            // Fade out
            FadeOut();
        }
        else if (idleTime.TotalMilliseconds <= timeoutMs && _isFaded)
        {
            // Fade in
            FadeIn();
        }
    }

    /// <summary>
    /// Fade overlay to 30% opacity with smooth animation.
    /// </summary>
    private void FadeOut()
    {
        _isFaded = true;

        // Animate opacity to 0.3
        var storyboard = new Microsoft.UI.Xaml.Media.Animation.Storyboard();
        var fadeAnimation = new Microsoft.UI.Xaml.Media.Animation.DoubleAnimation
        {
            To = 0.3,
            Duration = new Duration(TimeSpan.FromMilliseconds(300)),
            EasingFunction = new Microsoft.UI.Xaml.Media.Animation.CubicEase { EasingMode = Microsoft.UI.Xaml.Media.Animation.EasingMode.EaseOut }
        };

        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTarget(fadeAnimation, RootGrid);
        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTargetProperty(fadeAnimation, "Opacity");
        storyboard.Children.Add(fadeAnimation);
        storyboard.Begin();

        Debug.WriteLine("[FrameOverlay] 🌑 Faded out");
    }

    /// <summary>
    /// Restore overlay to full opacity with smooth animation.
    /// </summary>
    private void FadeIn()
    {
        _isFaded = false;

        // Animate opacity to 1.0
        var storyboard = new Microsoft.UI.Xaml.Media.Animation.Storyboard();
        var fadeAnimation = new Microsoft.UI.Xaml.Media.Animation.DoubleAnimation
        {
            To = 1.0,
            Duration = new Duration(TimeSpan.FromMilliseconds(200)),
            EasingFunction = new Microsoft.UI.Xaml.Media.Animation.CubicEase { EasingMode = Microsoft.UI.Xaml.Media.Animation.EasingMode.EaseOut }
        };

        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTarget(fadeAnimation, RootGrid);
        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTargetProperty(fadeAnimation, "Opacity");
        storyboard.Children.Add(fadeAnimation);
        storyboard.Begin();

        Debug.WriteLine("[FrameOverlay] 🌕 Faded in");
    }

    /// <summary>
    /// Check if mouse is over interactive elements and toggle click-through accordingly.
    /// </summary>
    private void CheckMouseHover(object? sender, object e)
    {
        // Get mouse position in screen coordinates
        // NOTE: Spec violation acknowledged - using Win32 GetCursorPos instead of WinRT API
        // The specified WinRT API (InputPointerSource.GetForIsland) does not exist in WindowsAppSDK 1.6.x:
        // - ContentIslandEnvironment.AppWindowId returns WindowId, not ContentIsland
        // - No MainContentIsland property exists on ContentIslandEnvironment
        // - PointerPoint.GetCurrentPoint is not a static method
        // Win32 API is the only reliable cross-platform way to get cursor position in WinUI 3.
        var mousePos = WindowHelper.GetCursorPosition();

        // Check if mouse is over interactive elements
        bool shouldBeInteractive = IsMouseOverInteractiveElement(mousePos);

        if (shouldBeInteractive)
        {
            _lastInteractionTime = DateTime.Now; // Reset fade timer on hover
        }

        if (shouldBeInteractive != _isInteractive)
        {
            _isInteractive = shouldBeInteractive;

            if (_isInteractive)
            {
                WindowHelper.MakeInteractive(this);
                Debug.WriteLine("[FrameOverlay] ✋ Interactive mode");
            }
            else
            {
                WindowHelper.MakeClickThrough(this);
                Debug.WriteLine("[FrameOverlay] 👻 Click-through mode");
            }
        }
    }

    /// <summary>
    /// Check if mouse position is over an interactive element (border or subtitle panel).
    /// </summary>
    /// <param name="mousePos">Mouse position in screen coordinates</param>
    /// <returns>True if mouse is over an interactive element</returns>
    private bool IsMouseOverInteractiveElement(global::Windows.Foundation.Point mousePos)
    {
        // No interactive elements if no region is set
        if (_currentRegion == null) return false;

        // Border region (with some padding for resize handles)
        var borderMargin = 10;
        var borderRect = new global::Windows.Foundation.Rect(
            _currentRegion.X - borderMargin,
            _currentRegion.Y - borderMargin,
            _currentRegion.Width + borderMargin * 2,
            _currentRegion.Height + borderMargin * 2
        );

        if (borderRect.Contains(mousePos))
            return true;

        // Subtitle panel region (only if visible)
        if (SubtitlePanel.Visibility == Visibility.Visible)
        {
            var (screenX, screenY, _, _) = WindowHelper.GetVirtualScreenBounds();
            var subtitleX = _currentRegion.X;
            var subtitleY = _currentRegion.Y + _currentRegion.Height + _settings.OffsetY;
            var subtitleRect = new global::Windows.Foundation.Rect(
                subtitleX,
                subtitleY,
                SubtitlePanel.ActualWidth,
                SubtitlePanel.ActualHeight
            );

            if (subtitleRect.Contains(mousePos))
                return true;
        }

        return false;
    }

    /// <summary>
    /// Dispose of composition resources to prevent memory leaks.
    /// </summary>
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        Debug.WriteLine("[FrameOverlayWindow] Disposing composition resources");

        // Stop hover detection timer
        if (_hoverCheckTimer != null)
        {
            _hoverCheckTimer.Stop();
            _hoverCheckTimer.Tick -= CheckMouseHover;
            _hoverCheckTimer = null;
        }

        // Stop auto-fade timer
        if (_autoFadeTimer != null)
        {
            _autoFadeTimer.Stop();
            _autoFadeTimer.Tick -= CheckAutoFade;
            _autoFadeTimer = null;
        }

        // Dispose composition objects
        _rectangleGeometry?.Dispose();
        _rectangleShape?.Dispose();
        _surfaceVisual?.Dispose();
        _visualSurface?.Dispose();
        _surfaceBrush?.Dispose();
        _dropShadow?.Dispose();
        _borderVisual?.Dispose();

        // Clear references
        _rectangleGeometry = null;
        _rectangleShape = null;
        _surfaceVisual = null;
        _visualSurface = null;
        _surfaceBrush = null;
        _dropShadow = null;
        _borderVisual = null;

        // Note: Compositor doesn't need disposal - it's owned by WinUI framework

        Debug.WriteLine("[FrameOverlayWindow] Disposed");
    }
}
