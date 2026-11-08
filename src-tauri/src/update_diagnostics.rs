//! Privacy-safe update metadata checks in the normal application session log.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckIntent {
    Manual,
    Automatic,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckPhase {
    Started,
    ManualTakeover,
    Finished,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckResult {
    UpToDate,
    Available,
    Failed,
    Timeout,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckErrorKind {
    Network,
    Timeout,
    Unknown,
}

/// Free-form errors, URLs, and screen text do not belong in this payload.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCheckDiagnostic {
    pub request_id: u32,
    pub source: UpdateCheckIntent,
    pub intent: UpdateCheckIntent,
    pub phase: UpdateCheckPhase,
    pub started_at_ms: u64,
    pub elapsed_ms: u64,
    pub result: Option<UpdateCheckResult>,
    pub error_kind: Option<UpdateCheckErrorKind>,
}

#[tauri::command]
pub fn record_update_check(event: UpdateCheckDiagnostic) {
    tracing::info!(
        request_id = event.request_id,
        source = ?event.source,
        intent = ?event.intent,
        phase = ?event.phase,
        started_at_ms = event.started_at_ms,
        elapsed_ms = event.elapsed_ms,
        result = ?event.result,
        error_kind = ?event.error_kind,
        "Update metadata check"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn finished_event() -> Value {
        json!({
            "requestId": 7,
            "source": "automatic",
            "intent": "manual",
            "phase": "finished",
            "startedAtMs": 1_700_000_000_000_u64,
            "elapsedMs": 10_000,
            "result": "timeout",
            "errorKind": "timeout"
        })
    }

    #[test]
    fn all_check_phases_accept_missing_outcomes() -> Result<(), serde_json::Error> {
        for phase in ["started", "manualTakeover", "finished"] {
            let mut value = finished_event();
            value["phase"] = json!(phase);
            if let Some(object) = value.as_object_mut() {
                object.remove("result");
                object.remove("errorKind");
            }
            let event: UpdateCheckDiagnostic = serde_json::from_value(value)?;
            assert!(event.result.is_none());
            assert!(event.error_kind.is_none());
        }
        Ok(())
    }

    #[test]
    fn free_form_diagnostics_are_rejected() {
        let mut value = finished_event();
        value["errorMessage"] = json!("private request details");
        assert!(serde_json::from_value::<UpdateCheckDiagnostic>(value).is_err());
        let mut value = finished_event();
        value["errorKind"] = json!("private request details");
        assert!(serde_json::from_value::<UpdateCheckDiagnostic>(value).is_err());
    }

    #[test]
    fn invalid_numeric_fields_are_rejected() {
        for field in ["requestId", "startedAtMs", "elapsedMs"] {
            let mut value = finished_event();
            value[field] = json!(-1);
            assert!(serde_json::from_value::<UpdateCheckDiagnostic>(value).is_err());
        }
    }
}
