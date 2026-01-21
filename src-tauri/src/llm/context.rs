// =============================================================================
// CONTEXT.RS - Session Translation Context
// =============================================================================
// Maintains translation memory and history for consistent translations.
// Context resets when capture stops and starts again.
// =============================================================================

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
    /// Original OCR text
    pub source: String,
    /// Translated result
    pub translation: String,
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
    /// Recent translation history (ring buffer, deduplicated)
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
            history: VecDeque::with_capacity(50),
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
            if Self::text_similarity(&last_entry.source, ocr_text) > DEDUP_SIMILARITY_THRESHOLD {
                return true;
            }
        }

        false
    }

    /// Add a translation to the history (call after successful translation)
    pub fn add_translation(&mut self, source: &str, translation: &str) {
        if !self.enabled {
            return;
        }

        let token_estimate = Self::estimate_tokens(source) + Self::estimate_tokens(translation);

        let entry = HistoryEntry {
            source: source.to_string(),
            translation: translation.to_string(),
            timestamp: Instant::now(),
            token_estimate,
        };

        // Update last OCR hash
        self.last_ocr_hash = Some(Self::hash_text(source));

        // Add to history
        self.history.push_back(entry);
        self.history_tokens += token_estimate;

        // Check if we need compression
        self.check_compression_threshold();
    }

    /// Build the context prompt to prepend to translation requests
    pub fn build_context_prompt(&self) -> Option<String> {
        self.build_context_prompt_with_recent_limit(3)
    }

    pub fn build_context_prompt_with_recent_limit(&self, recent_limit: usize) -> Option<String> {
        self.build_prompt(true, recent_limit)
    }

    /// Build a memory-only prompt (excludes recent history).
    pub fn build_memory_prompt(&self) -> Option<String> {
        self.build_prompt(false, 0)
    }

    fn build_prompt(&self, include_recent: bool, recent_limit: usize) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let budget = self.budget_tokens;
        let mut tokens_used = 0usize;
        let mut parts = Vec::new();

        // Add memory if present (already hard-capped, but still ensure we fit).
        if let Some(ref memory) = self.memory {
            let memory_part = format!("[Context: {}]", memory.trim());
            let memory_tokens = Self::estimate_tokens(&memory_part);
            if memory_tokens <= budget {
                parts.push(memory_part);
                tokens_used = tokens_used.saturating_add(memory_tokens);
            } else {
                // Extremely defensive: if even the capped memory can't fit, truncate again.
                let overhead_tokens = Self::estimate_tokens("[Context: ]");
                let available = budget.saturating_sub(overhead_tokens);
                let truncated = Self::truncate_to_token_budget(memory, available);
                if !truncated.is_empty() {
                    let candidate = format!("[Context: {}]", truncated);
                    let candidate_tokens = Self::estimate_tokens(&candidate);
                    if candidate_tokens <= budget {
                        parts.push(candidate);
                        tokens_used = tokens_used.saturating_add(candidate_tokens);
                    }
                }
            }
        }

        if include_recent {
            // Add recent history (try to include up to `recent_limit` entries, but stop at the token budget).
            let recent_count = recent_limit.min(self.history.len());
            if recent_count > 0 {
                let recent: Vec<_> = self.history.iter().rev().take(recent_count).collect();
                let mut selected = Vec::new();
                let mut best_block: Option<String> = None;

                for entry in recent.into_iter().rev() {
                    selected.push(format!("{} -> {}", entry.source, entry.translation));
                    let candidate = format!("[Recent: {}]", selected.join(" | "));
                    let candidate_tokens = Self::estimate_tokens(&candidate);
                    if tokens_used.saturating_add(candidate_tokens) <= budget {
                        best_block = Some(candidate);
                    } else {
                        selected.pop();
                        break;
                    }
                }

                if let Some(block) = best_block {
                    parts.push(block);
                }
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
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
        let cjk_count = text.chars().filter(|c| Self::is_cjk(*c)).count();
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
            if Self::is_cjk(ch) {
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
        if !prefix.chars().any(Self::is_cjk) {
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

    fn is_cjk(c: char) -> bool {
        matches!(c,
            '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
            '\u{3400}'..='\u{4DBF}' |   // CJK Extension A
            '\u{3040}'..='\u{309F}' |   // Hiragana
            '\u{30A0}'..='\u{30FF}' |   // Katakana
            '\u{AC00}'..='\u{D7AF}'     // Hangul
        )
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
        ctx.add_translation("Hello world", "Hola mundo");

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
        ctx.add_translation("Hello", "Hola");
        ctx.add_translation("World", "Mundo");

        let prompt = ctx.build_context_prompt();
        assert!(prompt.is_some());
        assert!(prompt.unwrap().contains("Recent:"));
    }

    #[test]
    fn test_disabled_context() {
        let mut ctx = TranslationContext::new(500, false);

        // When disabled, nothing should accumulate
        assert!(!ctx.is_duplicate("Hello"));
        ctx.add_translation("Hello", "Hola");
        assert!(ctx.history.is_empty());
        assert!(ctx.build_context_prompt().is_none());
    }

    #[test]
    fn test_memory_prompt_excludes_recent() {
        let mut ctx = TranslationContext::new(500, true);
        ctx.set_memory("Genre: drama. Names: X->Y".to_string());
        ctx.add_translation("Hello", "Hola");

        let prompt = ctx.build_memory_prompt().unwrap();
        assert!(prompt.contains("Context:"));
        assert!(!prompt.contains("Recent:"));
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
        let mut ctx = TranslationContext::new(100, true);

        // Add entries until we hit threshold
        for i in 0..20 {
            ctx.add_translation(
                &format!("Source text number {}", i),
                &format!("Translation number {}", i),
            );
        }

        // Should need compression now
        assert!(ctx.needs_compression());
    }
}
