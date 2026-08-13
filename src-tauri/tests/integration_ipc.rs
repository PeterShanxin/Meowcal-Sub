//! Integration tests for IPC communication
//!
//! These tests verify that the IPC protocol can be correctly serialized and
//! deserialized for communication with the WinUI3 OverlayHost.

use meowcal_sub::ipc::protocol::{
    IpcMessage, RegionData, SelectorResultPayload, SetRegionPayload, SettingsSyncPayload,
    SubtitleUpdatePayload,
};

#[test]
fn test_ipc_message_creation_without_payload() {
    // Test creating a simple message without payload
    let message = IpcMessage::new("Overlay.Show");
    assert_eq!(message.v, 1);
    assert_eq!(message.message_type, "Overlay.Show");
    assert!(message.payload.is_none());
    assert!(!message.id.is_empty());
}

#[test]
fn test_ipc_message_creation_with_payload() {
    // Test creating a message with a payload
    let payload = SubtitleUpdatePayload {
        text: "Translated text".to_string(),
        source_text: "Original text".to_string(),
        timestamp: "2025-01-26T12:00:00Z".to_string(),
        backend_used: Some("Local Translation Engine".to_string()),
    };

    let message = IpcMessage::with_payload("Subtitle.Update", payload);
    assert_eq!(message.v, 1);
    assert_eq!(message.message_type, "Subtitle.Update");
    assert!(message.payload.is_some());
}

#[test]
fn test_ipc_message_serialization() {
    // Test that a message can be serialized to JSON
    let message = IpcMessage::new("Overlay.Hide");
    let serialized = serde_json::to_string(&message).unwrap();

    // Verify it contains expected fields
    assert!(serialized.contains(r#""v":1"#));
    assert!(serialized.contains(r#""type":"Overlay.Hide""#));
    assert!(serialized.contains(r#""id":"#));
}

#[test]
fn test_ipc_message_deserialization() {
    // Test that a message can be deserialized from JSON
    let json = r#"{
        "v": 1,
        "type": "Overlay.Show",
        "id": "test-id-123",
        "payload": null
    }"#;

    let message: IpcMessage = serde_json::from_str(json).unwrap();
    assert_eq!(message.v, 1);
    assert_eq!(message.message_type, "Overlay.Show");
    assert_eq!(message.id, "test-id-123");
    assert!(message.payload.is_none());
}

#[test]
fn test_ipc_message_with_payload_serialization() {
    // Test serialization with actual payload
    let payload = RegionData {
        x: 100,
        y: 200,
        width: 800,
        height: 600,
        coord_space: "physical".to_string(),
        monitor_id: None,
    };

    let message = IpcMessage::with_payload("Overlay.SetRegion", payload);
    let serialized = serde_json::to_string(&message).unwrap();

    assert!(serialized.contains(r#""type":"Overlay.SetRegion""#));
    assert!(serialized.contains(r#""x":100"#));
    assert!(serialized.contains(r#""y":200"#));
    assert!(serialized.contains(r#""width":800"#));
    assert!(serialized.contains(r#""height":600"#));
}

#[test]
fn test_ipc_message_with_payload_deserialization() {
    // Test deserialization with payload
    let json = r#"{
        "v": 1,
        "type": "Subtitle.Update",
        "id": "msg-456",
        "payload": {
            "text": "Hello",
            "sourceText": "Hola",
            "timestamp": "2025-01-26T12:00:00Z",
            "backendUsed": "Local Translation Engine"
        }
    }"#;

    let message: IpcMessage = serde_json::from_str(json).unwrap();
    assert_eq!(message.message_type, "Subtitle.Update");
    assert!(message.payload.is_some());

    // Extract and verify payload
    let payload = message.payload.unwrap();
    assert_eq!(payload["text"], "Hello");
    assert_eq!(payload["sourceText"], "Hola");
    assert_eq!(payload["backendUsed"], "Local Translation Engine");
}

#[test]
fn test_region_data_serialization() {
    // Test RegionData with camelCase conversion
    let region = RegionData {
        x: 100,
        y: 200,
        width: 800,
        height: 600,
        coord_space: "physical".to_string(),
        monitor_id: Some("MONITOR1".to_string()),
    };

    let serialized = serde_json::to_string(&region).unwrap();
    assert!(serialized.contains(r#""x":100"#));
    assert!(serialized.contains(r#""y":200"#));
    assert!(serialized.contains(r#""width":800"#));
    assert!(serialized.contains(r#""height":600"#));
    assert!(serialized.contains(r#""coordSpace":"physical""#));
    assert!(serialized.contains(r#""monitorId":"MONITOR1""#));
}

#[test]
fn test_subtitle_update_payload_serialization() {
    // Test SubtitleUpdatePayload with camelCase conversion
    let payload = SubtitleUpdatePayload {
        text: "Translated".to_string(),
        source_text: "Original".to_string(),
        timestamp: "2025-01-26T12:00:00Z".to_string(),
        backend_used: Some("Local Translation Engine".to_string()),
    };

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains(r#""text":"Translated""#));
    assert!(serialized.contains(r#""sourceText":"Original""#));
    assert!(serialized.contains(r#""timestamp":"2025-01-26T12:00:00Z""#));
    assert!(serialized.contains(r#""backendUsed":"Local Translation Engine""#));
}

#[test]
fn test_subtitle_update_payload_optional_fields() {
    // Test that optional fields are skipped when None
    let payload = SubtitleUpdatePayload {
        text: "Translated".to_string(),
        source_text: "Original".to_string(),
        timestamp: "2025-01-26T12:00:00Z".to_string(),
        backend_used: None,
    };

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(!serialized.contains("backendUsed"));
}

#[test]
fn test_selector_result_payload_serialization() {
    // Test SelectorResultPayload with nested region
    let region = RegionData {
        x: 50,
        y: 50,
        width: 1920,
        height: 1080,
        coord_space: "physical".to_string(),
        monitor_id: Some("PRIMARY".to_string()),
    };

    let payload = SelectorResultPayload {
        region_physical: region,
        source_monitor: Some("PRIMARY".to_string()),
        dpi: 1.5,
    };

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains(r#""regionPhysical""#));
    assert!(serialized.contains(r#""sourceMonitor":"PRIMARY""#));
    assert!(serialized.contains(r#""dpi":1.5"#));
}

#[test]
fn test_overlay_settings_data_serialization() {
    // Test OverlaySettingsData with multiple camelCase fields
    let settings = meowcal_sub::ipc::protocol::OverlaySettingsData {
        font_size: 24,
        font_family: "Arial".to_string(),
        text_color: "#FFFFFF".to_string(),
        background_color: "#000000".to_string(),
        offset_y: 100,
        max_width: 800,
        auto_fade_timeout_ms: 3000,
        border_color: "#00A8FF".to_string(),
        border_width: 3,
    };

    let serialized = serde_json::to_string(&settings).unwrap();
    assert!(serialized.contains(r#""fontSize":24"#));
    assert!(serialized.contains(r#""fontFamily":"Arial""#));
    assert!(serialized.contains(r##""textColor":"#FFFFFF""##));
    assert!(serialized.contains(r##""backgroundColor":"#000000""##));
    assert!(serialized.contains(r#""offsetY":100"#));
    assert!(serialized.contains(r#""maxWidth":800"#));
    assert!(serialized.contains(r#""autoFadeTimeoutMs":3000"#));
    assert!(serialized.contains(r##""borderColor":"#00A8FF""##));
    assert!(serialized.contains(r#""borderWidth":3"#));
}

#[test]
fn test_settings_sync_payload_serialization() {
    // Test SettingsSyncPayload with nested overlay settings
    let overlay_settings = meowcal_sub::ipc::protocol::OverlaySettingsData {
        font_size: 20,
        font_family: "Segoe UI".to_string(),
        text_color: "#000000".to_string(),
        background_color: "#FFFFFF".to_string(),
        offset_y: 50,
        max_width: 1000,
        auto_fade_timeout_ms: 5000,
        border_color: "#FF0000".to_string(),
        border_width: 2,
    };

    let payload = SettingsSyncPayload {
        overlay: overlay_settings,
    };

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains(r#""overlay""#));
    assert!(serialized.contains(r#""fontSize":20"#));
    assert!(serialized.contains(r#""fontFamily":"Segoe UI""#));
}

#[test]
fn test_ipc_message_round_trip() {
    // Test that a message can be serialized and deserialized without data loss
    let original_payload = RegionData {
        x: 100,
        y: 200,
        width: 800,
        height: 600,
        coord_space: "physical".to_string(),
        monitor_id: Some("MONITOR1".to_string()),
    };

    let original = IpcMessage::with_payload("Overlay.SetRegion", original_payload);
    let serialized = serde_json::to_string(&original).unwrap();
    let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();

    assert_eq!(original.v, deserialized.v);
    assert_eq!(original.message_type, deserialized.message_type);
    assert_eq!(original.id, deserialized.id);

    // Verify payload is preserved
    let deserialized_payload = deserialized.payload.unwrap();
    assert_eq!(deserialized_payload["x"], 100);
    assert_eq!(deserialized_payload["y"], 200);
    assert_eq!(deserialized_payload["width"], 800);
    assert_eq!(deserialized_payload["height"], 600);
}

#[test]
fn test_set_region_payload_wrapper() {
    // Test SetRegionPayload wrapper structure
    let region = RegionData {
        x: 10,
        y: 20,
        width: 300,
        height: 400,
        coord_space: "physical".to_string(),
        monitor_id: None,
    };

    let payload = SetRegionPayload { region };
    let serialized = serde_json::to_string(&payload).unwrap();

    assert!(serialized.contains(r#""region""#));
    assert!(serialized.contains(r#""x":10"#));
    assert!(serialized.contains(r#""y":20"#));
    assert!(serialized.contains(r#""width":300"#));
    assert!(serialized.contains(r#""height":400"#));
}

#[test]
fn test_ipc_message_unique_ids() {
    // Test that each message gets a unique UUID
    let msg1 = IpcMessage::new("Overlay.Show");
    let msg2 = IpcMessage::new("Overlay.Hide");

    assert_ne!(msg1.id, msg2.id);
}

#[test]
fn test_ipc_message_with_complex_payload() {
    // Test message with a complex nested payload
    let region = RegionData {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        coord_space: "physical".to_string(),
        monitor_id: Some("PRIMARY".to_string()),
    };

    let selector_payload = SelectorResultPayload {
        region_physical: region,
        source_monitor: Some("PRIMARY".to_string()),
        dpi: 1.5,
    };

    let message = IpcMessage::with_payload("Selector.Result", selector_payload);
    let serialized = serde_json::to_string(&message).unwrap();

    // Verify the complex structure is serialized correctly
    assert!(serialized.contains(r#""type":"Selector.Result""#));
    assert!(serialized.contains(r#""regionPhysical""#));
    assert!(serialized.contains(r#""dpi":1.5"#));
    assert!(serialized.contains(r#""sourceMonitor":"PRIMARY""#));
}
