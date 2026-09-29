use super::prompt_router::clean_source_text;
use std::collections::VecDeque;

const RECENT_TRANSLATIONS: usize = 12;
const MAX_ENTRY_CHARS: usize = 4_000;

struct Entry {
    source: String,
    translated: String,
    source_language: String,
    target_language: String,
}

/// Session-local comparison evidence, never included in prompts or persisted.
#[derive(Default)]
pub(super) struct RecentTranslations {
    entries: VecDeque<Entry>,
    generation: u64,
}

impl RecentTranslations {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn repeats_other_source(
        &self,
        source: &str,
        translated: &str,
        source_language: &str,
        target_language: &str,
    ) -> bool {
        let source = normalize(source);
        let translated = normalize(translated);
        self.entries.iter().any(|entry| {
            entry.source != source
                && entry.source_language.eq_ignore_ascii_case(source_language)
                && entry.target_language.eq_ignore_ascii_case(target_language)
                && contains_phrase(&translated, &entry.translated)
        })
    }

    pub(super) fn record(
        &mut self,
        source: &str,
        translated: &str,
        source_language: &str,
        target_language: &str,
        generation: u64,
    ) {
        if generation != self.generation {
            return;
        }
        // Never turn a truncated target into a match against an unrelated phrase.
        if source.chars().count() > MAX_ENTRY_CHARS || translated.chars().count() > MAX_ENTRY_CHARS
        {
            return;
        }
        let source = normalize(source);
        let translated = normalize(translated);
        if source.is_empty() || translated.is_empty() {
            return;
        }
        while self.entries.len() >= RECENT_TRANSLATIONS {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            source,
            translated,
            source_language: source_language.to_string(),
            target_language: target_language.to_string(),
        });
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.generation = self.generation.wrapping_add(1);
    }
}

fn normalize(text: &str) -> String {
    let text: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    clean_source_text(&text)
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    // A short CJK answer such as "是" is ordinary text inside "是的".
    // Only a whole-answer match justifies confirmation for these short cues.
    if phrase.chars().count() < 4 && phrase.chars().any(super::text_utils::is_cjk_char) {
        return text == phrase;
    }
    !phrase.is_empty()
        && text.match_indices(phrase).any(|(start, matched)| {
            let end = start + matched.len();
            // Respect Unicode word boundaries without requiring spaces between CJK cues.
            let begins_word = phrase.starts_with(is_word_char);
            let ends_word = phrase.ends_with(is_word_char);
            (!begins_word || !text[..start].ends_with(is_word_char))
                && (!ends_word || !text[end..].starts_with(is_word_char))
        })
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() && !super::text_utils::is_cjk_compactable_char(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_leakage_matches_punctuation_variants_and_complete_latin_words() {
        let mut history = RecentTranslations::default();
        history.record("One cue", "房间是空的。", "en", "zh", 0);
        assert!(history.repeats_other_source(
            "Another cue",
            "房间是空的！还有十分钟。",
            "en",
            "zh"
        ));
        history.record("没有", "No.", "zh", "en", 0);
        assert!(!history.repeats_other_source("无人知道", "Nobody knows.", "zh", "en"));
        assert!(history.repeats_other_source("他拒绝", "No, he refused.", "zh", "en"));
        assert!(!history.repeats_other_source("没有！", "No!", "zh", "en"));
    }

    #[test]
    fn context_leakage_history_is_bounded_and_never_matches_empty_output() {
        let mut history = RecentTranslations::default();
        history.record("Oldest", "最早", "en", "zh", 0);
        for index in 0..RECENT_TRANSLATIONS {
            history.record(
                &format!("Line {index}"),
                &format!("译文{index}"),
                "en",
                "zh",
                0,
            );
        }
        assert!(!history.repeats_other_source("New line", "最早", "en", "zh"));
        history.record("Empty", "...", "en", "zh", 0);
        assert!(!history.repeats_other_source("New line", "新的译文", "en", "zh"));
        history.clear();
        assert!(!history.repeats_other_source("New line", "译文11", "en", "zh"));
    }

    #[test]
    fn context_leakage_requires_complete_accented_words() {
        let mut history = RecentTranslations::default();
        history.record("To", "À", "en", "fr", 0);
        assert!(!history.repeats_other_source("Here it is", "Voilà", "en", "fr"));
        assert!(history.repeats_other_source("At home", "À la maison", "en", "fr"));
        history.record("Born", "Né", "en", "fr", 0);
        assert!(!history.repeats_other_source("A cinema", "Un cinéma", "en", "fr"));
        assert!(history.repeats_other_source("Born here", "Né ici", "en", "fr"));
        history.record("Summer", "Été", "en", "fr", 0);
        assert!(!history.repeats_other_source("Repeated", "Répété", "en", "fr"));
        assert!(history.repeats_other_source("The summer", "L'été", "en", "fr"));
    }
}
