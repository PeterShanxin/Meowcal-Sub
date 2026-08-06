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
///
/// Measured against the longer of the two reads, not both. A re-read that drops
/// a glyph is the common case - `击碎她的信仰` came back as `击的信仰` - and
/// gating on the shorter read meant exactly those pairs skipped the similarity
/// check and were translated twice.
const MIN_CHARS_FOR_SIMILARITY: usize = 4;

/// How much a read has to grow before the extra text is worth retranslating.
///
/// A subtitle that is still drawing gains a clause; OCR resolving one more
/// stroke gains a character. Across a 45-minute session every read that grew by
/// three characters or fewer was noise - `活下去` became `艹活下去 0 艹`,
/// `斗转星移` became `0 斗转星移` - and each one replaced a good translation on
/// screen with a different one. A genuine second row arrives with far more.
const EXTENDED_MIN_NEW_CHARS: usize = 4;

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
    //
    // Only when what is left is most of the line, though. Normalising strips
    // spaces and punctuation, so a two-word reply is contained in plenty of
    // unrelated dialogue - `No.` is inside `I don't know.`, `Wait.` inside
    // `I told you to wait outside` - and treating those as re-reads silently
    // swallowed the short exchanges that make up much of a script.
    if contains_subsequence(&previous, &current) && current.len() * 2 >= previous.len() {
        return LineChange::Repeat;
    }

    // Growth is only worth retranslating when it carries a clause. A stray
    // glyph resolved at either edge of the strip grows the read too, and
    // retranslating on it swaps a good line on screen for a different one.
    if contains_subsequence(&current, &previous) {
        return if current.len() - previous.len() >= EXTENDED_MIN_NEW_CHARS {
            LineChange::Extended
        } else {
            LineChange::Repeat
        };
    }

    if previous.len().max(current.len()) >= MIN_CHARS_FOR_SIMILARITY
        && similarity(&previous, &current) >= SAME_LINE_SIMILARITY
    {
        return LineChange::Repeat;
    }

    LineChange::New
}

/// Reduce a read to the characters that carry meaning. Spacing and punctuation
/// are the first things OCR gets wrong and the last things worth retranslating
/// over.
pub(crate) fn normalize(text: &str) -> Vec<char> {
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
pub(crate) fn similarity(a: &[char], b: &[char]) -> f32 {
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

    // A re-read that lost a glyph lands under the old six-character floor, so
    // the similarity check never ran and the pair was translated twice. Taken
    // from a 0.6.3 session where each of these put a second English rendering
    // of one line on screen.
    #[test]
    fn a_re_read_that_shrank_below_the_floor_is_still_the_same_line() {
        for (previous, current) in [
            ("击碎她的信仰", "击的信仰"),
            ("然后祈蝉量过", "然后祈蝉过"),
            ("你真是不懂啊", "你不懂啊"),
            ("这真是开心啊", "这真是心啊"),
            ("杂种小姑娘高", "杂种小娘高"),
            ("仅此而已啊", "匕而已啊"),
            ("难遣是你吗", "难道是你吗"),
            ("我之所选你", "我之所以选你"),
            ("如此一来", "如仳来"),
        ] {
            assert_eq!(
                classify(previous, current),
                LineChange::Repeat,
                "{previous} -> {current}"
            );
        }
    }

    // Windows resolves a letterbox edge or a half-drawn stroke into a stray
    // glyph, which grows the read without adding anything to translate. Every
    // one of these was classified Extended and replaced a good line on screen
    // with a differently-worded one.
    #[test]
    fn a_read_that_grew_by_a_stray_glyph_is_not_worth_retranslating() {
        for (previous, current) in [
            ("活下去", "艹活下去 0 艹"),
            ("斗转星移", "0 斗转星移"),
            ("种资格啊", "我亠种资格啊"),
            ("有不好的东西在靠近", "卜有不好的东西在靠近。"),
            ("厉害好厉害", "好厉害好厉害"),
            ("我要引擎全开了吉", "我要引擎全开了吉尔"),
            ("但这个选择自己做的", "但这个选择自己做的判断"),
            ("View: Category", "View: Category 0"),
        ] {
            assert_eq!(
                classify(previous, current),
                LineChange::Repeat,
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
