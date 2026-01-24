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
public sealed partial class FrameOverlayWindow : Window
{
    private AppWindow? _appWindow;
    private Region? _currentRegion;
    private OverlaySettings _settings = new(); // Use default settings if not yet synced
    private Compositor? _compositor;
    private SpriteVisual? _borderVisual;

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
    /// <param name="original">Original OCR text (currently unused)</param>
    /// <param name="translated">Translated subtitle text to display</param>
    /// <param name="backendUsed">Translation backend name (currently unused)</param>
    public void UpdateSubtitle(string original, string translated, string backendUsed)
    {
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
    /// </summary>
    private async void SettingsButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Debug.WriteLine("[FrameOverlayWindow] Settings button clicked");

            // TODO: Send message to backend to open settings
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
        // Dispose existing border visual
        if (_borderVisual != null)
        {
            _borderVisual.Dispose();
            _borderVisual = null;
        }

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

        // Attach to overlay canvas
        _borderVisual = spriteVisual;
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
}
