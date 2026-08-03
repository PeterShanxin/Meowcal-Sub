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
/// same line.
///
/// Calibrated against a real zh-to-en episode rather than guessed. Across 63
/// translated lines, consecutive reads of one subtitle scored 0.50 to 0.85,
/// while genuinely different dialogue never exceeded 0.25 - Windows OCR is
/// unstable enough on a subtitle strip that a re-read commonly differs by
/// several characters, not by one. An earlier 0.85 sat inside the same-line
/// band and let 26 of those 63 translations through as fresh dialogue, which is
/// what put two and three renderings of one line on screen.
const SAME_LINE_SIMILARITY: f32 = 0.45;

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

    // Consecutive reads taken verbatim from a zh-to-en episode log. These are
    // the pairs that reached the translator twice, so the threshold has to
    // separate them from the genuinely new dialogue below.
    #[test]
    fn real_re_reads_from_an_episode_are_repeats() {
        for (previous, current) in [
            (
                "那么彳尔有为此而杀死对方的觉悟吗",
                "阝么你有为此而杀死对方的觉悟吗",
            ),
            ("我也很想很惠看看其它英难", "我也很相很看看其它英难"),
            ("如果有难道友好相处", "如果有机难道不想友好相处巾"),
            ("如果我能和七位都成为朋友", "如果我能和七位英难都成为朋友"),
            ("那样京征服世界也不甫是梦啊", "那样就服世界也不再梦啊"),
            (
                "我能成为教授的弟子真是三生有幸",
                "我能成为教授自真是三生有幸",
            ),
        ] {
            assert_eq!(
                classify(previous, current),
                LineChange::Repeat,
                "{previous} -> {current}"
            );
        }
    }

    #[test]
    fn real_line_changes_from_an_episode_are_new() {
        for (previous, current) in [
            ("如果有机会唯道不想相处吗", "如果我自孬七亻雄都成为朋友"),
            ("真的真的很感谢你", "我能成为教授的弟子真是三生有幸"),
            (
                "圣杯就是抱有这番觉悟的人们所追求的东西吧",
                "这不就更想一探究竟吗",
            ),
            ("沦落到比死还惨的境界最后还一事无成", "甚至会被残忍杀害"),
            (
                "那么你有为此而杀死对方的悟吗一艹",
                "有没有不杀对疠也能的办法",
            ),
        ] {
            assert_eq!(
                classify(previous, current),
                LineChange::New,
                "{previous} -> {current}"
            );
        }
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
