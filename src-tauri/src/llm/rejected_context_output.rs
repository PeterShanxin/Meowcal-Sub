use std::fmt;

/// An overlong contextual answer awaiting replay classification.
#[derive(Clone)]
pub struct RejectedContextOutput {
    pub(super) output: String,
    receipt: Option<String>,
}

impl std::error::Error for RejectedContextOutput {}

impl RejectedContextOutput {
    pub(super) fn new(output: String, receipt: Option<String>) -> Self {
        Self { output, receipt }
    }

    pub(super) fn report(&self) {
        if let Some(receipt) = &self.receipt {
            crate::core_client::recover_inference(receipt.clone(), "too_long");
        }
    }
}

impl fmt::Display for RejectedContextOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Translation output rejected as corrupted (overlong output).")
    }
}

// Errors can reach diagnostics through both formatting traits. Neither may
// expose the subtitle text or the inference receipt retained for recovery.
impl fmt::Debug for RejectedContextOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RejectedContextOutput")
            .field("output_chars", &self.output.chars().count())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_formatting_does_not_expose_rejected_text_or_receipts() {
        let error = crate::llm::LlmError::from(RejectedContextOutput::new(
            "private subtitle".into(),
            Some("private receipt".into()),
        ));
        for rendered in [format!("{error}"), format!("{error:?}")] {
            assert!(!rendered.contains("private"));
        }
        assert_eq!(error.code(), "translation_error");
        assert!(!crate::llm::transport_errors::is_transient(&error));
    }
}
