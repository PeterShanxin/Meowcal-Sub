// =============================================================================
// OCR_STABILITY.RS - Deciding when OCR is showing a new subtitle line
// =============================================================================
// A subtitle sits on screen for two to four seconds while the capture loop
// reads it ten or more times. Those reads are not identical: a glyph drops, a
// comma turns into a period, a line renders half-drawn for one frame. Compared
// by exact string equality - which is all the pipeline did whenever
// context-aware translation was off, and it is off by default - each variation
// looks like fresh dialogue and earns its own trip through the translator.
//
// That is what puts two or three different English renderings of one Chinese
// line on screen in a row, and it is also why some of them read badly: a
// half-drawn line translated on its own is a fragment, not a sentence.
// =============================================================================

/// How the current OCR read relates to the last line that was translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    /// The same line read again, give or take OCR noise. Nothing to translate.
    Repeat,
    /// The same line with more of it visible - a second row appearing, or a
    /// line that was still drawing. Worth retranslating: it carries text the
    /// previous read did not have.
    Extended,
    /// Different dialogue.
    New,
}

/// Below this many significant characters a single differing character is a
/// change of meaning rather than noise, so similarity is not consulted.
const MIN_CHARS_FOR_SIMILARITY: usize = 6;

/// Share of characters that must agree before two reads are treated as the
/// same line. Consecutive subtitles rarely resemble each other this closely;
/// re-reads of one line rarely fall below it.
const SAME_LINE_SIMILARITY: f32 = 0.85;

/// Classify the current OCR text against the last line that was translated.
pub fn classify(previous: &str, current: &str) -> LineChange {
    let previous = normalize(previous);
    let current = normalize(current);

    if previous.is_empty() {
        return LineChange::New;
    }
    if previous == current {
        return LineChange::Repeat;
    }

    // A read that lost characters is OCR dropping them, not dialogue getting
    // shorter. Suppressing it keeps the overlay from flickering back to a
    // partial version of a line it already showed in full.
    if contains_subsequence(&previous, &current) {
        return LineChange::Repeat;
    }

    if contains_subsequence(&current, &previous) {
        return LineChange::Extended;
    }

    if previous.len() >= MIN_CHARS_FOR_SIMILARITY
        && current.len() >= MIN_CHARS_FOR_SIMILARITY
        && similarity(&previous, &current) >= SAME_LINE_SIMILARITY
    {
        return LineChange::Repeat;
    }

    LineChange::New
}

/// Reduce a read to the characters that carry meaning. Spacing and punctuation
/// are the first things OCR gets wrong and the last things worth retranslating
/// over.
fn normalize(text: &str) -> Vec<char> {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

/// Whether `haystack` contains `needle` as a contiguous run, with `needle`
/// strictly shorter.
fn contains_subsequence(haystack: &[char], needle: &[char]) -> bool {
    if needle.is_empty() || needle.len() >= haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Share of characters shared by two reads, by edit distance.
fn similarity(a: &[char], b: &[char]) -> f32 {
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let distance = edit_distance(a, b);
    1.0 - (distance as f32 / longest as f32)
}

/// Levenshtein distance over two rows. Subtitle lines are short enough that the
/// quadratic cost is irrelevant next to a single OCR pass.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous_row: Vec<usize> = (0..=b.len()).collect();
    let mut current_row = vec![0usize; b.len() + 1];

    for (i, a_char) in a.iter().enumerate() {
        current_row[0] = i + 1;
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous_row[j] + usize::from(a_char != b_char);
            let deletion = previous_row[j + 1] + 1;
            let insertion = current_row[j] + 1;
            current_row[j + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous_row, &mut current_row);
    }

    previous_row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identical_read_is_a_repeat() {
        assert_eq!(classify("我们回家吧", "我们回家吧"), LineChange::Repeat);
    }

    #[test]
    fn punctuation_and_spacing_do_not_make_a_new_line() {
        assert_eq!(
            classify("Are you coming home?", "Are you coming home."),
            LineChange::Repeat
        );
        assert_eq!(
            classify("Are you coming home", "Arayou  coming home"),
            LineChange::Repeat
        );
    }

    // The defect this module exists for: one static subtitle read three
    // slightly different ways became three translations on screen.
    #[test]
    fn a_single_dropped_glyph_is_the_same_line() {
        assert_eq!(
            classify("这件事我们明天再谈吧", "这件事我们明天再谈"),
            LineChange::Repeat
        );
        assert_eq!(
            classify("这件事我们明天再谈吧", "这件事我明天再谈吧"),
            LineChange::Repeat
        );
    }

    #[test]
    fn a_read_that_lost_the_end_of_the_line_is_not_retranslated() {
        assert_eq!(
            classify("你现在就得走不然赶不上了", "你现在就得走"),
            LineChange::Repeat
        );
    }

    // A second row appearing carries text the first read did not have, so it
    // has to reach the translator.
    #[test]
    fn a_line_that_grew_is_worth_translating_again() {
        assert_eq!(
            classify("你现在就得走", "你现在就得走不然赶不上了"),
            LineChange::Extended
        );
    }

    #[test]
    fn different_dialogue_is_new() {
        assert_eq!(classify("我们回家吧", "他明天才到"), LineChange::New);
        assert_eq!(
            classify("Are you coming home?", "I left the keys inside."),
            LineChange::New
        );
    }

    // Short lines carry too little to average over: one character is the whole
    // meaning.
    #[test]
    fn short_lines_are_compared_strictly() {
        assert_eq!(classify("好的", "好吗"), LineChange::New);
        assert_eq!(classify("不行", "不行"), LineChange::Repeat);
    }

    #[test]
    fn the_first_line_of_a_session_is_new() {
        assert_eq!(classify("", "我们回家吧"), LineChange::New);
    }

    #[test]
    fn a_read_that_went_blank_is_new_rather_than_a_repeat() {
        // An empty read is filtered earlier in the pipeline; if one arrives
        // here it must not be mistaken for the previous line.
        assert_eq!(classify("我们回家吧", ""), LineChange::New);
    }
}
