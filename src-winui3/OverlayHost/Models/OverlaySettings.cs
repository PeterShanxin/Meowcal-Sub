using System.Text.Json.Serialization;

namespace OverlayHost.Models;

/// <summary>
/// Settings for overlay appearance and behavior.
/// Synced from backend via Settings.Sync message.
/// </summary>
public record OverlaySettings
{
    [JsonPropertyName("fontSize")]
    public int FontSize { get; init; } = 24;

    [JsonPropertyName("fontFamily")]
    public string FontFamily { get; init; } = "Segoe UI";

    [JsonPropertyName("textColor")]
    public string TextColor { get; init; } = "#FFFFFF";

    [JsonPropertyName("backgroundColor")]
    public string BackgroundColor { get; init; } = "rgba(0,0,0,0.7)";

    [JsonPropertyName("offsetY")]
    public int OffsetY { get; init; } = 10;

    [JsonPropertyName("maxWidth")]
    public int MaxWidth { get; init; } = 0; // 0 = match capture region width

    [JsonPropertyName("autoFadeTimeoutMs")]
    public int AutoFadeTimeoutMs { get; init; } = 3000;

    [JsonPropertyName("borderColor")]
    public string BorderColor { get; init; } = "#00A8FF"; // Accent color

    [JsonPropertyName("borderWidth")]
    public int BorderWidth { get; init; } = 3;
}
