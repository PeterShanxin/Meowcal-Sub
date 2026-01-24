using System.Text.Json;
using System.Text.Json.Serialization;

namespace OverlayHost.Models;

/// <summary>
/// Base IPC message structure for Named Pipe communication.
/// All messages follow the versioned JSON protocol.
/// </summary>
public record IpcMessage
{
    /// <summary>
    /// Protocol version (currently 1)
    /// </summary>
    [JsonPropertyName("v")]
    public int Version { get; init; } = 1;

    /// <summary>
    /// Message type (e.g., "Region.Set", "Subtitle.Update")
    /// </summary>
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    /// <summary>
    /// Unique message ID (UUID)
    /// </summary>
    [JsonPropertyName("id")]
    public string Id { get; init; } = Guid.NewGuid().ToString();

    /// <summary>
    /// Message payload (type-specific data)
    /// </summary>
    [JsonPropertyName("payload")]
    public JsonElement? Payload { get; init; }

    /// <summary>
    /// Create a typed message with payload
    /// </summary>
    public static IpcMessage Create<T>(string type, T payload)
    {
        return new IpcMessage
        {
            Type = type,
            Payload = JsonSerializer.SerializeToElement(payload)
        };
    }

    /// <summary>
    /// Deserialize payload to specific type
    /// </summary>
    public T? GetPayload<T>()
    {
        if (Payload == null)
            return default;

        return Payload.Value.Deserialize<T>();
    }
}

// ==================== Payload Types ====================

/// <summary>
/// Payload for Region.Set message - updates the capture region
/// </summary>
public record SetRegionPayload
{
    [JsonPropertyName("region")]
    public required Region Region { get; init; }
}

/// <summary>
/// Payload for Subtitle.Update message - new translated subtitle text
/// </summary>
public record SubtitleUpdatePayload
{
    [JsonPropertyName("text")]
    public required string Text { get; init; }

    /// <summary>
    /// Optional source text (OCR output before translation)
    /// </summary>
    [JsonPropertyName("sourceText")]
    public string? SourceText { get; init; }

    /// <summary>
    /// Optional timestamp (ISO 8601)
    /// </summary>
    [JsonPropertyName("timestamp")]
    public string? Timestamp { get; init; }
}

/// <summary>
/// Payload for Selector.Result message - area selection completed
/// </summary>
public record SelectorResultPayload
{
    /// <summary>
    /// Selected region (null if selection was cancelled)
    /// </summary>
    [JsonPropertyName("region")]
    public Region? Region { get; init; }

    /// <summary>
    /// Whether selection was cancelled
    /// </summary>
    [JsonPropertyName("cancelled")]
    public bool Cancelled { get; init; }
}

/// <summary>
/// Payload for Settings.Sync message - overlay settings update
/// </summary>
public record SettingsSyncPayload
{
    [JsonPropertyName("settings")]
    public required OverlaySettings Settings { get; init; }
}

/// <summary>
/// Payload for Capture.Status message - capture state updates
/// </summary>
public record CaptureStatusPayload
{
    /// <summary>
    /// Capture state: "idle", "running", "paused", "error"
    /// </summary>
    [JsonPropertyName("state")]
    public required string State { get; init; }

    /// <summary>
    /// Optional error message if state is "error"
    /// </summary>
    [JsonPropertyName("error")]
    public string? Error { get; init; }
}
