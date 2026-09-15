//! Windows declared with `"create": false` in `tauri.conf.json` are built on
//! first use, so a launch that never selects an area or opens setup does not
//! start their WebView2 renderers.

use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

/// The window with `label`, built from its configuration if it does not exist yet.
///
/// Call this from async commands only: on Windows, building a webview window
/// from a synchronous command deadlocks.
pub fn get_or_create(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == label)
        .ok_or_else(|| format!("No window named {label} in tauri.conf.json"))?;
    WebviewWindowBuilder::from_config(app, config)
        .and_then(|builder| builder.build())
        // A second open that raced this one may have built the window first.
        .or_else(|error| {
            app.get_webview_window(label)
                .ok_or_else(|| error.to_string())
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_main_and_overlay_windows_start_with_the_app() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json should parse");
        let at_launch: Vec<&str> = config["app"]["windows"]
            .as_array()
            .expect("tauri.conf.json should declare windows")
            .iter()
            .filter(|window| window["create"].as_bool().unwrap_or(true))
            .filter_map(|window| window["label"].as_str())
            .collect();

        // The overlay stays: its liveness recovery treats a page that has not
        // announced readiness as wedged and reloads it.
        assert_eq!(at_launch, ["main", "overlay"]);
    }
}
