use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// IPC message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    /// Protocol version
    pub v: u32,
    /// Message type (e.g., "Overlay.Show")
    #[serde(rename = "type")]
    pub message_type: String,
    /// Unique message ID
    pub id: String,
    /// Message payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl IpcMessage {
    pub fn new(message_type: impl Into<String>) -> Self {
        Self {
            v: 1,
            message_type: message_type.into(),
            id: Uuid::new_v4().to_string(),
            payload: None,
        }
    }

    pub fn with_payload<T: Serialize>(message_type: impl Into<String>, payload: T) -> Self {
        Self {
            v: 1,
            message_type: message_type.into(),
            id: Uuid::new_v4().to_string(),
            payload: Some(serde_json::to_value(payload).unwrap()),
        }
    }
}

/// Payload for Overlay.SetRegion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRegionPayload {
    pub region: RegionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionData {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub coord_space: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_id: Option<String>,
}

impl From<&crate::config::CaptureRegion> for RegionData {
    fn from(region: &crate::config::CaptureRegion) -> Self {
        Self {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
            coord_space: "physical".to_string(),
            monitor_id: None,
        }
    }
}

/// Payload for Subtitle.Update
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleUpdatePayload {
    pub original: String,
    pub translated: String,
    pub timestamp: u64,
    pub backend_used: String,
}

/// Payload for Settings.Sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSyncPayload {
    pub overlay: OverlaySettingsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySettingsData {
    pub font_size: u32,
    pub font_family: String,
    pub text_color: String,
    pub background_color: String,
    pub offset_y: i32,
    pub max_width: u32,
    pub auto_fade_timeout_ms: u32,
    pub border_color: String,
    pub border_width: u32,
}

impl From<&crate::config::OverlayConfig> for OverlaySettingsData {
    fn from(config: &crate::config::OverlayConfig) -> Self {
        Self {
            font_size: config.font_size,
            font_family: config.font_family.clone(),
            text_color: config.text_color.clone(),
            background_color: config.background_color.clone(),
            offset_y: config.offset_y,
            max_width: config.max_width,
            auto_fade_timeout_ms: 3000, // Default
            border_color: "#00A8FF".to_string(), // Default accent color
            border_width: 3,
        }
    }
}

/// Payload for Selector.Result (from OverlayHost)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorResultPayload {
    pub region_physical: RegionData,
    pub source_monitor: Option<String>,
    pub dpi: f64,
}
