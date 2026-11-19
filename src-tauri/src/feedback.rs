//! Immutable, locally previewed feedback snapshots. Only the main webview can request them.
pub(crate) mod session_log;
pub(crate) mod trace;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, LazyLock, Mutex};
use trace::{Frame, Stage};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    id: String,
    kind: &'static str,
    label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_data_url: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    snapshot_id: String,
    version: &'static str,
    stages: Vec<Stage>,
    materials: Vec<Material>,
}

fn text_material(kind: &'static str, label: &'static str, text: String) -> Material {
    Material {
        id: kind.to_owned(),
        kind,
        label,
        text: Some(text),
        image_data_url: None,
    }
}

impl Preview {
    fn from_frame(frame: Option<Frame>) -> Self {
        let mut materials = Vec::new();
        let mut stages = Vec::new();
        if let Some(frame) = frame {
            if let Some((lines, path)) = session_log::excerpt(&frame) {
                materials.push(text_material("log", "Session log excerpt", lines));
                materials.push(text_material("path", "Session log path", path));
            }
            if let Some(pair) = frame.translation {
                materials.push(text_material("translation", "Source and translation", pair));
            }
            if let Some(recognized) = frame.recognized {
                materials.push(text_material("ocr", "Recognized text", recognized));
            }
            if let Some(png) = frame.image {
                materials.push(Material {
                    id: "screenshot".into(),
                    kind: "screenshot",
                    label: "Capture region",
                    text: None,
                    image_data_url: Some(format!(
                        "data:image/png;base64,{}",
                        STANDARD.encode(png.as_ref())
                    )),
                });
            }
            stages = frame.stages;
        }
        Self {
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            version: env!("CARGO_PKG_VERSION"),
            stages,
            materials,
        }
    }

    fn files(&self, ids: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
        let selected: HashSet<&String> = ids.iter().collect();
        if selected.len() != ids.len()
            || selected.len() > self.materials.len()
            || selected
                .iter()
                .any(|id| !self.materials.iter().any(|m| &m.id == *id))
        {
            return Err("FEEDBACK_INVALID_SELECTION".into());
        }
        let summary = serde_json::to_vec_pretty(
            &serde_json::json!({"version":self.version,"stages":self.stages}),
        )
        .map_err(|_| "FEEDBACK_ENCODING_FAILED".to_owned())?;
        let mut files = vec![("diagnostics.json".into(), summary)];
        for item in &self.materials {
            if !selected.contains(&item.id) {
                continue;
            }
            let (name, data) = match (&item.text, &item.image_data_url) {
                (Some(text), None) => (format!("{}.txt", item.kind), text.as_bytes().to_vec()),
                (None, Some(image)) => {
                    let data = image
                        .strip_prefix("data:image/png;base64,")
                        .and_then(|s| STANDARD.decode(s).ok())
                        .ok_or("FEEDBACK_ENCODING_FAILED")?;
                    ("capture.png".into(), data)
                }
                _ => return Err("FEEDBACK_ENCODING_FAILED".into()),
            };
            files.push((name, data));
        }
        Ok(files)
    }
}

#[derive(Default)]
struct Snapshots {
    fault: VecDeque<Arc<Preview>>,
    general: VecDeque<Arc<Preview>>,
}

impl Snapshots {
    fn save(&mut self, preview: Arc<Preview>, fault: bool) {
        // A fault stays actionable while the user opens unrelated general reports.
        let lane = if fault {
            &mut self.fault
        } else {
            &mut self.general
        };
        if lane.len() == 4 {
            lane.pop_front();
        }
        lane.push_back(preview);
    }
    fn get(&self, id: &str) -> Option<Arc<Preview>> {
        self.general
            .iter()
            .chain(&self.fault)
            .find(|p| p.snapshot_id == id)
            .cloned()
    }
}

static PREVIEWS: LazyLock<Mutex<Snapshots>> = LazyLock::new(Mutex::default);

pub(crate) fn require_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("FEEDBACK_WINDOW_DENIED".into())
    }
}

fn error_frame(code: &str) -> Frame {
    let (stage, code) = match code {
        "OCR_FAILED" => ("ocr", "OCR_FAILED"),
        "OCR_INIT_FAILED" => ("ocr", "OCR_INIT_FAILED"),
        "CAPTURE_FAILED" => ("capture", "CAPTURE_FAILED"),
        "DISPLAY_CHANGED" => ("capture", "DISPLAY_CHANGED"),
        "ENGINE_NOT_INSTALLED" => ("engine", "ENGINE_NOT_INSTALLED"),
        "ENGINE_MODEL_MISSING" => ("engine", "ENGINE_MODEL_MISSING"),
        "ENGINE_INCOMPATIBLE" => ("engine", "ENGINE_INCOMPATIBLE"),
        "ENGINE_MANIFEST_INVALID" => ("engine", "ENGINE_MANIFEST_INVALID"),
        "ENGINE_UNSUPPORTED_ARCH" => ("engine", "ENGINE_UNSUPPORTED_ARCH"),
        "ENGINE_DISK_SPACE" => ("engine", "ENGINE_DISK_SPACE"),
        "ENGINE_ARTIFACT_INTEGRITY" => ("engine", "ENGINE_ARTIFACT_INTEGRITY"),
        "ENGINE_ROLLBACK_REJECTED" => ("engine", "ENGINE_ROLLBACK_REJECTED"),
        "ENGINE_SAMPLE_TRANSLATION_FAILED" => ("engine", "ENGINE_SAMPLE_TRANSLATION_FAILED"),
        other if other.starts_with("ENGINE_") => ("engine", "ENGINE_ERROR"),
        _ => ("app", "APP_ERROR"),
    };
    Frame::application_error(stage, code)
}

#[tauri::command]
pub async fn feedback_preview(
    window: tauri::WebviewWindow,
    fault_code: Option<String>,
    fault_token: Option<String>,
) -> Result<Preview, String> {
    require_main(&window)?;
    let is_fault = fault_code.is_some();
    let frame = trace::select(fault_code.as_deref(), fault_token.as_deref())
        .or_else(|| fault_code.as_deref().map(error_frame));
    tokio::task::spawn_blocking(move || {
        let preview = Arc::new(Preview::from_frame(frame));
        crate::sync_utils::lock_or_recover(&PREVIEWS).save(Arc::clone(&preview), is_fault);
        (*preview).clone()
    })
    .await
    .map_err(|_| "FEEDBACK_PREVIEW_FAILED".into())
}

pub(crate) fn selected_files(
    snapshot_id: &str,
    material_ids: &[String],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let preview = crate::sync_utils::lock_or_recover(&PREVIEWS)
        .get(snapshot_id)
        .ok_or("FEEDBACK_PREVIEW_EXPIRED")?;
    preview.files(material_ids)
}

#[tauri::command]
pub fn feedback_overlay_received(
    window: tauri::WebviewWindow,
    session_id: u64,
    capture_id: u64,
) -> Result<(), String> {
    if window.label() != "overlay" {
        return Err("FEEDBACK_WINDOW_DENIED".into());
    }
    trace::overlay_received(crate::pipeline_session::PipelineToken {
        session_id,
        capture_id,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_reports_cannot_evict_the_current_fault() {
        let mut cache = Snapshots::default();
        let fault = Arc::new(Preview::from_frame(Some(error_frame("APP_ERROR"))));
        cache.save(Arc::clone(&fault), true);
        for _ in 0..9 {
            cache.save(Arc::new(Preview::from_frame(None)), false);
        }
        assert!(Arc::ptr_eq(&fault, &cache.get(&fault.snapshot_id).unwrap()));
        assert_eq!(cache.general.len(), 4);
    }

    #[test]
    fn a_verbose_frame_exports_only_what_the_preview_shows() {
        let mut scene = Frame::application_error("translation", "TRANSLATION_FAILED");
        let png = b"opaque test image";
        scene.image = Some(std::sync::Arc::from(&png[..]));
        scene.recognized = Some("secret OCR line".to_owned());
        scene.translation = Some("line A\ntranslation B".to_owned());
        let preview = Preview::from_frame(Some(scene));
        let default_files = preview.files(&[]).unwrap();
        let summary = String::from_utf8(default_files[0].1.clone()).unwrap();
        assert_eq!(default_files.len(), 1);
        for private in [
            "opaque test image",
            "secret OCR line",
            "translation B",
            "data:image",
            "materials",
        ] {
            assert!(!summary.contains(private));
        }
        let selected = preview
            .files(&["screenshot".into(), "translation".into()])
            .unwrap();
        let zip_entries = selected
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(zip_entries["capture.png"], png);
        let translation = preview
            .materials
            .iter()
            .find(|m| m.kind == "translation")
            .unwrap();
        assert_eq!(
            zip_entries["translation.txt"],
            translation.text.as_ref().unwrap().as_bytes()
        );
        let image_preview = preview
            .materials
            .iter()
            .find(|m| m.kind == "screenshot")
            .unwrap()
            .image_data_url
            .as_ref()
            .unwrap();
        assert_eq!(
            image_preview,
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(&zip_entries["capture.png"])
            )
        );
        assert!(!zip_entries.contains_key("ocr.txt"));
    }

    #[test]
    fn only_selected_immutable_materials_can_reach_archive() {
        let mut preview = Preview::from_frame(Some(error_frame("ENGINE_FAKE_SECRET")));
        preview.materials.push(text_material(
            "ocr",
            "Recognized text",
            "private subtitle".into(),
        ));
        preview.materials.push(text_material(
            "path",
            "Session log path",
            "C:/Users/private".into(),
        ));
        let summary = preview.files(&[]).unwrap();
        assert_eq!(summary.len(), 1);
        let s = String::from_utf8(summary[0].1.clone()).unwrap();
        for private in ["private", "SECRET", "Recognized text", "Users"] {
            assert!(!s.contains(private));
        }
        let chosen = preview.files(&["ocr".into()]).unwrap();
        assert_eq!(
            chosen[1],
            ("ocr.txt".to_owned(), b"private subtitle".to_vec())
        );
        for invalid in [vec!["../x".into()], vec!["ocr".into(), "ocr".into()]] {
            assert!(preview.files(&invalid).is_err());
        }
    }
}
