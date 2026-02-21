// =============================================================================
// CONTEXT.RS - Session Translation Context
// =============================================================================
// Maintains translation memory and history for consistent translations.
// Context resets when capture stops and starts again.
// =============================================================================

use super::text_utils::is_cjk_char;
use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::Instant;

/// Default context budget as percentage of model's context window
const DEFAULT_CONTEXT_BUDGET_PERCENT: f32 = 0.15;
/// Minimum token budget (fallback if model detection fails)
const MIN_TOKEN_BUDGET: usize = 200;
/// Maximum token budget cap
const MAX_TOKEN_BUDGET: usize = 2000;
/// Threshold percentage to trigger memory compression
const COMPRESSION_THRESHOLD_PERCENT: f32 = 0.7;
/// Similarity threshold for deduplication (0.0-1.0)
const DEDUP_SIMILARITY_THRESHOLD: f32 = 0.85;
/// Hard cap for summarized memory (tokens, estimated).
const MAX_MEMORY_TOKENS: usize = 300;
/// Maximum share of the context budget that memory can consume.
/// Keeps room for recent history and prevents runaway prompts.
const MEMORY_BUDGET_PERCENT: f32 = 0.35;

/// A single translation history entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Original OCR text (store source only to avoid error propagation)
    pub text: String,
    /// When this was captured
    pub timestamp: Instant,
    /// Estimated token count for this entry
    pub token_estimate: usize,
}

/// Session-scoped translation context
#[derive(Debug)]
pub struct TranslationContext {
    /// Compressed memory from LLM summarization (names, genre, key facts)
    memory: Option<String>,
    /// Recent OCR history (ring buffer, deduplicated)
    history: VecDeque<HistoryEntry>,
    /// Hash of last OCR text for fast duplicate detection
    last_ocr_hash: Option<u64>,
    /// Total estimated tokens in history
    history_tokens: usize,
    /// Token budget for context (auto-detected or default)
    budget_tokens: usize,
    /// Whether context is enabled
    enabled: bool,
    /// Pending summarization flag
    needs_compression: bool,
}

impl TranslationContext {
    /// Create a new context with the given token budget
    pub fn new(budget_tokens: usize, enabled: bool) -> Self {
        let budget = budget_tokens.clamp(MIN_TOKEN_BUDGET, MAX_TOKEN_BUDGET);
        Self {
            memory: None,
            history: VecDeque::with_capacity(12),
            last_ocr_hash: None,
            history_tokens: 0,
            budget_tokens: budget,
            enabled,
            needs_compression: false,
        }
    }

    /// Create context with auto-detected budget from model context window
    pub fn with_model_context_window(context_window: usize, enabled: bool) -> Self {
        let budget = ((context_window as f32) * DEFAULT_CONTEXT_BUDGET_PERCENT) as usize;
        Self::new(budget, enabled)
    }

    /// Check if the given OCR text is a duplicate of the last one
    pub fn is_duplicate(&self, ocr_text: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let current_hash = Self::hash_text(ocr_text);

        // Fast path: exact hash match
        if let Some(last_hash) = self.last_ocr_hash {
            if current_hash == last_hash {
                return true;
            }
        }

        // Check similarity with recent entries
        if let Some(last_entry) = self.history.back() {
            let normalized_last = Self::normalize_for_dedup(&last_entry.text);
            let normalized_current = Self::normalize_for_dedup(ocr_text);

            // If OCR is progressively revealing more characters, treat it as a new line so we can
            // translate the updated text (avoid skipping "superset" updates).
            if normalized_current.contains(&normalized_last)
                && normalized_current.chars().count() > normalized_last.chars().count()
            {
                return false;
            }

            // If OCR regressed (subset), consider it duplicate to avoid flicker.
            if normalized_last.contains(&normalized_current)
                && normalized_last.chars().count() > normalized_current.chars().count()
            {
                return true;
            }

            if Self::text_similarity(&last_entry.text, ocr_text) > DEDUP_SIMILARITY_THRESHOLD {
                return true;
            }
        }

        false
    }

    /// Record an OCR line into the rolling cache (call after OCR, before translation).
    ///
    /// Stores source-only (no translated output) to prevent error propagation.
    pub fn add_ocr_line(
        &mut self,
        source_text: &str,
        now: Instant,
        max_entries: usize,
        max_entry_chars: usize,
        reset_gap: std::time::Duration,
    ) {
        if !self.enabled {
            return;
        }

        let cleaned = source_text.trim();
        if cleaned.is_empty() {
            return;
        }

        if !reset_gap.is_zero() {
            if let Some(last) = self.history.back() {
                if now.duration_since(last.timestamp) > reset_gap {
                    self.reset();
                }
            }
        }

        let normalized_new = Self::normalize_for_dedup(cleaned);
        if Self::is_noise_line(&normalized_new) {
            return;
        }

        // De-jitter: update timestamp if duplicate-ish, and replace last entry if the new line is a strict superset.
        if let Some(last_entry) = self.history.back_mut() {
            let normalized_last = Self::normalize_for_dedup(&last_entry.text);

            if normalized_new == normalized_last {
                last_entry.timestamp = now;
                self.last_ocr_hash = Some(Self::hash_text(&last_entry.text));
                return;
            }

            if normalized_new.contains(&normalized_last)
                && normalized_new.chars().count() > normalized_last.chars().count()
            {
                let clipped = Self::clip_chars(cleaned, max_entry_chars);
                self.history_tokens = self
                    .history_tokens
                    .saturating_sub(last_entry.token_estimate);
                last_entry.text = clipped;
                last_entry.timestamp = now;
                last_entry.token_estimate = Self::estimate_tokens(&last_entry.text);
                self.history_tokens += last_entry.token_estimate;
                self.last_ocr_hash = Some(Self::hash_text(&last_entry.text));
                self.check_compression_threshold();
                return;
            }

            if normalized_last.contains(&normalized_new)
                && normalized_last.chars().count() > normalized_new.chars().count()
            {
                // OCR regression (lost characters): keep the longer previous line but refresh timestamp.
                last_entry.timestamp = now;
                self.last_ocr_hash = Some(Self::hash_text(&last_entry.text));
                return;
            }
        }

        let clipped = Self::clip_chars(cleaned, max_entry_chars);
        let token_estimate = Self::estimate_tokens(&clipped);

        while self.history.len() >= max_entries.max(1) {
            if let Some(entry) = self.history.pop_front() {
                self.history_tokens = self.history_tokens.saturating_sub(entry.token_estimate);
            } else {
                break;
            }
        }

        self.history.push_back(HistoryEntry {
            text: clipped,
            timestamp: now,
            token_estimate,
        });
        self.history_tokens += token_estimate;

        // Update last OCR hash
        self.last_ocr_hash = Some(Self::hash_text(cleaned));

        // Check if we need compression
        self.check_compression_threshold();
    }

    /// Build the context prompt to prepend to translation requests
    pub fn build_context_prompt(&self) -> Option<String> {
        self.build_context_prompt_with_recent_limit(3, 600)
    }

    pub fn build_context_prompt_with_recent_limit(
        &self,
        recent_limit: usize,
        max_context_chars: usize,
    ) -> Option<String> {
        self.build_prompt(true, recent_limit, max_context_chars)
    }

    /// Build a memory-only prompt (excludes recent history).
    pub fn build_memory_prompt(&self, max_context_chars: usize) -> Option<String> {
        self.build_prompt(false, 0, max_context_chars)
    }

    fn build_prompt(
        &self,
        include_recent: bool,
        recent_limit: usize,
        max_context_chars: usize,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }

        if max_context_chars == 0 {
            return None;
        }

        let memory_line = self
            .memory
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let mut recent_lines: Vec<String> = Vec::new();
        if include_recent {
            let recent_count = recent_limit.min(self.history.len());
            if recent_count > 0 {
                recent_lines = self
                    .history
                    .iter()
                    .rev()
                    .take(recent_count)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|entry| entry.text.clone())
                    .collect();
            }
        }

        let mut memory_lines: Vec<String> = Vec::new();
        if let Some(line) = memory_line {
            memory_lines.push(line);
        }

        // Enforce max_context_chars by dropping oldest recent lines first.
        loop {
            let total_chars = Self::joined_char_count(&memory_lines, &recent_lines);
            if total_chars <= max_context_chars {
                break;
            }

            if !recent_lines.is_empty() {
                recent_lines.remove(0);
                continue;
            }

            if !memory_lines.is_empty() {
                let available = max_context_chars;
                memory_lines[0] = Self::clip_chars(&memory_lines[0], available);
                break;
            }

            break;
        }

        let mut lines = Vec::new();
        lines.extend(memory_lines);
        lines.extend(recent_lines);

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    /// Check if compression is needed and set the flag
    fn check_compression_threshold(&mut self) {
        let threshold = (self.budget_tokens as f32 * COMPRESSION_THRESHOLD_PERCENT) as usize;
        if self.history_tokens > threshold {
            self.needs_compression = true;
        }
    }

    /// Returns true if the context needs compression
    pub fn needs_compression(&self) -> bool {
        self.needs_compression && self.enabled
    }

    /// Get history for summarization (consumes entries)
    pub fn get_history_for_summarization(&mut self) -> Vec<HistoryEntry> {
        // Keep last 3 entries, return the rest for summarization
        let keep_count = 3.min(self.history.len());
        let drain_count = self.history.len().saturating_sub(keep_count);

        if drain_count == 0 {
            return Vec::new();
        }

        let entries: Vec<_> = self.history.drain(..drain_count).collect();

        // Recalculate token count
        self.recalculate_history_tokens();
        self.needs_compression = false;

        entries
    }

    /// Restore drained history entries (oldest first)
    pub fn restore_history_entries(&mut self, entries: Vec<HistoryEntry>) {
        if entries.is_empty() {
            return;
        }

        for entry in entries.into_iter().rev() {
            self.history.push_front(entry);
        }

        self.recalculate_history_tokens();
    }

    /// Trim history to stay within the budget (accounting for memory)
    pub fn cap_history_to_budget(&mut self) {
        if !self.enabled {
            return;
        }

        let memory_tokens = self
            .memory
            .as_ref()
            .map(|m| Self::estimate_tokens(m))
            .unwrap_or(0);
        let budget = self.budget_tokens.saturating_sub(memory_tokens);

        self.recalculate_history_tokens();

        while self.history_tokens > budget {
            if let Some(entry) = self.history.pop_front() {
                self.history_tokens = self.history_tokens.saturating_sub(entry.token_estimate);
            } else {
                break;
            }
        }

        self.needs_compression = false;
    }

    /// Update memory with summarized content
    pub fn set_memory(&mut self, memory: String) {
        if !self.enabled {
            return;
        }

        let trimmed = memory.trim();
        if trimmed.is_empty() {
            self.memory = None;
            return;
        }

        let budget = self.memory_token_budget();
        let truncated = Self::truncate_to_token_budget(trimmed, budget);
        if truncated.is_empty() {
            self.memory = None;
        } else {
            self.memory = Some(truncated);
        }

        // Ensure we still fit the overall budget after updating memory.
        self.cap_history_to_budget();
    }

    /// Get current memory content (for debugging/diagnostics)
    pub fn memory(&self) -> Option<&str> {
        self.memory.as_deref()
    }

    /// Get history length
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get current token usage
    pub fn token_usage(&self) -> (usize, usize) {
        let memory_tokens = self
            .memory
            .as_ref()
            .map(|m| Self::estimate_tokens(m))
            .unwrap_or(0);
        (self.history_tokens + memory_tokens, self.budget_tokens)
    }

    /// Reset context (called when session ends)
    pub fn reset(&mut self) {
        self.memory = None;
        self.history.clear();
        self.last_ocr_hash = None;
        self.history_tokens = 0;
        self.needs_compression = false;
    }

    fn joined_char_count(memory_lines: &[String], recent_lines: &[String]) -> usize {
        let mut total = 0usize;
        let mut line_count = 0usize;
        for line in memory_lines.iter().chain(recent_lines.iter()) {
            let chars = line.chars().count();
            if chars == 0 {
                continue;
            }
            if line_count > 0 {
                total += 1; // newline
            }
            total += chars;
            line_count += 1;
        }
        total
    }

    fn clip_chars(text: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }

        if text.chars().count() <= max_chars {
            return text.to_string();
        }

        text.chars().take(max_chars).collect::<String>()
    }

    fn normalize_for_dedup(text: &str) -> String {
        let collapsed = text
            .split_whitespace()
            .filter(|chunk| !chunk.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let trimmed = collapsed.trim();
        trimmed
            .trim_end_matches(|ch: char| Self::is_trailing_punct(ch))
            .to_string()
    }

    fn is_trailing_punct(ch: char) -> bool {
        matches!(
            ch,
            '.' | '!'
                | '?'
                | ','
                | ';'
                | ':'
                | '。'
                | '！'
                | '？'
                | '，'
                | '；'
                | '：'
                | '…'
                | '、'
                | '·'
        )
    }

    fn is_noise_line(text: &str) -> bool {
        let significant = text
            .chars()
            .filter(|ch| ch.is_alphanumeric() || is_cjk_char(*ch))
            .count();
        significant < 2
    }

    /// Simple hash for duplicate detection
    fn hash_text(text: &str) -> u64 {
        let normalized = text.trim().to_lowercase();
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        hasher.finish()
    }

    /// Estimate tokens (rough: ~4 chars per token for CJK-heavy content)
    fn estimate_tokens(text: &str) -> usize {
        // CJK characters are roughly 1 token each, ASCII ~4 chars per token
        let cjk_count = text.chars().filter(|c| is_cjk_char(*c)).count();
        let total_chars = text.chars().count();
        let non_cjk_count = total_chars.saturating_sub(cjk_count);
        cjk_count + non_cjk_count.div_ceil(4)
    }

    fn memory_token_budget(&self) -> usize {
        let percent_budget = ((self.budget_tokens as f32) * MEMORY_BUDGET_PERCENT) as usize;
        percent_budget
            .clamp(MIN_TOKEN_BUDGET / 5, MAX_MEMORY_TOKENS)
            .min(self.budget_tokens)
    }

    fn truncate_to_token_budget(text: &str, max_tokens: usize) -> String {
        if max_tokens == 0 {
            return String::new();
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        if Self::estimate_tokens(trimmed) <= max_tokens {
            return trimmed.to_string();
        }

        let mut cjk_count = 0usize;
        let mut non_cjk_count = 0usize;
        let mut end = 0usize;

        for (idx, ch) in trimmed.char_indices() {
            if is_cjk_char(ch) {
                cjk_count += 1;
            } else {
                non_cjk_count += 1;
            }

            let tokens = cjk_count + non_cjk_count.div_ceil(4);
            if tokens > max_tokens {
                break;
            }

            end = idx + ch.len_utf8();
        }

        let mut prefix = trimmed[..end].trim().to_string();
        if prefix.is_empty() {
            return prefix;
        }

        // For ASCII-heavy text, try to avoid cutting mid-word.
        if !prefix.chars().any(is_cjk_char) {
            if let Some(pos) = prefix.rfind(char::is_whitespace) {
                let candidate = prefix[..pos].trim();
                if !candidate.is_empty() {
                    prefix = candidate.to_string();
                }
            }
        }

        prefix
    }

    fn recalculate_history_tokens(&mut self) {
        self.history_tokens = self.history.iter().map(|e| e.token_estimate).sum();
    }

    /// Simple Jaccard-ish similarity for deduplication
    fn text_similarity(a: &str, b: &str) -> f32 {
        let a_norm = a.trim().to_lowercase();
        let b_norm = b.trim().to_lowercase();

        if a_norm == b_norm {
            return 1.0;
        }

        if a_norm.is_empty() || b_norm.is_empty() {
            return 0.0;
        }

        // Character-level Jaccard similarity
        let a_chars: std::collections::HashSet<char> = a_norm.chars().collect();
        let b_chars: std::collections::HashSet<char> = b_norm.chars().collect();

        let intersection = a_chars.intersection(&b_chars).count();
        let union = a_chars.union(&b_chars).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_context() {
        let ctx = TranslationContext::new(500, true);
        assert_eq!(ctx.budget_tokens, 500);
        assert!(ctx.enabled);
        assert!(ctx.memory.is_none());
        assert!(ctx.history.is_empty());
    }

    #[test]
    fn test_duplicate_detection() {
        let mut ctx = TranslationContext::new(500, true);

        // First text is not a duplicate
        assert!(!ctx.is_duplicate("Hello world"));

        // Add it to history
        ctx.add_ocr_line(
            "Hello world",
            Instant::now(),
            12,
            300,
            Duration::from_secs(10),
        );

        // Same text should be duplicate
        assert!(ctx.is_duplicate("Hello world"));

        // Similar text should be duplicate
        assert!(ctx.is_duplicate("hello world"));

        // Different text should not be duplicate
        assert!(!ctx.is_duplicate("Goodbye world"));
    }

    #[test]
    fn test_context_prompt() {
        let mut ctx = TranslationContext::new(500, true);

        // No context initially
        assert!(ctx.build_context_prompt().is_none());

        // Add some history
        let now = Instant::now();
        ctx.add_ocr_line("Hello", now, 12, 300, Duration::from_secs(10));
        ctx.add_ocr_line("World", now, 12, 300, Duration::from_secs(10));

        let prompt = ctx.build_context_prompt();
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("World"));
    }

    #[test]
    fn test_disabled_context() {
        let mut ctx = TranslationContext::new(500, false);

        // When disabled, nothing should accumulate
        assert!(!ctx.is_duplicate("Hello"));
        ctx.add_ocr_line("Hello", Instant::now(), 12, 300, Duration::from_secs(10));
        assert!(ctx.history.is_empty());
        assert!(ctx.build_context_prompt().is_none());
    }

    #[test]
    fn test_memory_prompt_excludes_recent() {
        let mut ctx = TranslationContext::new(500, true);
        ctx.set_memory("Genre: drama. Names: X->Y".to_string());
        ctx.add_ocr_line("Hello", Instant::now(), 12, 300, Duration::from_secs(10));

        let prompt = ctx.build_memory_prompt(600).unwrap();
        assert!(prompt.contains("Genre:"));
        assert!(!prompt.contains("Hello"));
    }

    #[test]
    fn test_memory_truncation_hard_cap() {
        let mut ctx = TranslationContext::new(200, true);
        let long = "a".repeat(2000);
        ctx.set_memory(long);

        let mem = ctx.memory().unwrap_or_default();
        let budget = ctx.memory_token_budget();
        assert!(TranslationContext::estimate_tokens(mem) <= budget);
    }

    #[test]
    fn test_token_estimation() {
        // ASCII text
        let ascii_tokens = TranslationContext::estimate_tokens("Hello world");
        assert!(ascii_tokens > 0 && ascii_tokens < 10);

        // CJK text (each char ~1 token)
        let cjk_tokens = TranslationContext::estimate_tokens("你好世界");
        assert_eq!(cjk_tokens, 4);
    }

    #[test]
    fn test_compression_threshold() {
        // Budget is clamped to MIN_TOKEN_BUDGET (200), so ensure we exceed the threshold.
        let mut ctx = TranslationContext::new(200, true);

        // Add entries until we hit threshold
        for i in 0..40 {
            ctx.add_ocr_line(
                &format!("Source text number {}", i),
                Instant::now(),
                100,
                300,
                Duration::from_secs(10),
            );
        }

        // Should need compression now
        assert!(ctx.needs_compression());
    }
}
