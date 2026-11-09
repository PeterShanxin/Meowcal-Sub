use super::output_validation::{quality_issue_message, TranslationOutputRejection};
use std::fmt;

/// Rejected output awaiting a bounded retry or contextual replay classification.
#[derive(Clone)]
pub struct RejectedOutput {
    pub(super) output: String,
    pub(super) reason: TranslationOutputRejection,
    receipt: Option<String>,
}

impl std::error::Error for RejectedOutput {}

impl RejectedOutput {
    pub(super) fn new(
        output: String,
        receipt: Option<String>,
        reason: TranslationOutputRejection,
    ) -> Self {
        Self {
            output,
            receipt,
            reason,
        }
    }

    pub(super) fn report(&self) {
        if let Some(receipt) = &self.receipt {
            crate::core_client::recover_inference(receipt.clone(), self.reason.code());
        }
    }
}

impl fmt::Display for RejectedOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&quality_issue_message(self.reason))
    }
}

// Errors can reach diagnostics through both formatting traits. Neither may
// expose the subtitle text or the inference receipt retained for recovery.
impl fmt::Debug for RejectedOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RejectedOutput")
            .field("reason", &self.reason)
            .field("output_chars", &self.output.chars().count())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_formatting_does_not_expose_rejected_text_or_receipts() {
        let error = crate::llm::LlmError::from(RejectedOutput::new(
            "private subtitle".into(),
            Some("private receipt".into()),
            TranslationOutputRejection::TooLong,
        ));
        for rendered in [format!("{error}"), format!("{error:?}")] {
            assert!(!rendered.contains("private"));
        }
        assert_eq!(error.code(), "translation_error");
        assert!(!crate::llm::transport_errors::is_transient(&error));
    }
}
