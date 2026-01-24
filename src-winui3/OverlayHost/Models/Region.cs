using System.Text.Json.Serialization;

namespace OverlayHost.Models;

/// <summary>
/// Represents a rectangular region in screen coordinates.
/// CRITICAL: All coordinates are in PHYSICAL PIXELS in virtual screen space.
/// </summary>
public record Region
{
    [JsonPropertyName("x")]
    public int X { get; init; }

    [JsonPropertyName("y")]
    public int Y { get; init; }

    [JsonPropertyName("width")]
    public int Width { get; init; }

    [JsonPropertyName("height")]
    public int Height { get; init; }

    /// <summary>
    /// Coordinate space identifier. Should always be "physical" for backend communication.
    /// </summary>
    [JsonPropertyName("coordSpace")]
    public string CoordSpace { get; init; } = "physical";

    /// <summary>
    /// Optional monitor identifier (e.g., "\\.\DISPLAY1")
    /// </summary>
    [JsonPropertyName("monitorId")]
    public string? MonitorId { get; init; }
}
