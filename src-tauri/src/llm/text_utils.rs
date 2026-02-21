// =============================================================================
// TEXT_UTILS.RS - Shared Unicode text helpers for LLM modules
// =============================================================================

/// Returns true when `ch` belongs to common CJK writing ranges used by OCR/translation logic.
pub fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Extension A
        '\u{3040}'..='\u{309F}' |   // Hiragana
        '\u{30A0}'..='\u{30FF}' |   // Katakana
        '\u{AC00}'..='\u{D7AF}'     // Hangul
    )
}

/// Returns true when `ch` is a Hangul syllable.
pub fn is_hangul_char(ch: char) -> bool {
    matches!(ch, '\u{AC00}'..='\u{D7AF}')
}

/// CJK scripts where OCR often inserts spaces between characters that should be removed.
///
/// Hangul is intentionally excluded because Korean uses spaces between words.
pub fn is_cjk_compactable_char(ch: char) -> bool {
    is_cjk_char(ch) && !is_hangul_char(ch)
}
