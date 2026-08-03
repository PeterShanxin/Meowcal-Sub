// =============================================================================
// TEXT_CLEANUP.RS - Repairing what Windows OCR hands back
// =============================================================================
// `OcrLine::Text` joins the recognised words with a space. For Latin script
// that is the sentence; for Chinese and Japanese, where Windows treats nearly
// every glyph as its own word, it is not. A subtitle came back as
// "亻 每 里 解 什 么 是 魔 术 师" and went to the translator that way, which is not
// a sentence in any language and is translated accordingly.
//
// Windows also resolves noise at the edge of a capture region - a letterbox
// border, the corner of a logo - into a stray character, usually a lone digit.
// Those turned up inside translated dialogue as a bare "0".
// =============================================================================

/// Whether a character belongs to a script written without spaces between
/// words: CJK ideographs, kana, and the Hangul syllables.
fn is_scriptio_continua(ch: char) -> bool {
    matches!(ch as u32,
        0x3000..=0x303F      // CJK punctuation: 。 、 「 」
        | 0x3040..=0x30FF    // hiragana, katakana
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified ideographs
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0xF900..=0xFAFF    // CJK compatibility ideographs
        | 0xFF00..=0xFF65    // fullwidth forms
    )
}

/// Drop the spaces Windows inserts between characters of a script that does not
/// use them, while leaving the spacing of Latin text untouched.
///
/// A space is removed only when the characters on both sides of it are
/// space-free script, so "第 3 集" keeps its digit spacing decisions to the
/// caller and "Hello 世界" keeps the space that separates the two scripts.
pub fn join_scriptio_continua(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());

    for (index, &ch) in characters.iter().enumerate() {
        if ch == ' ' {
            let before = characters[..index].iter().rev().find(|c| **c != ' ');
            let after = characters[index + 1..].iter().find(|c| **c != ' ');
            if let (Some(&before), Some(&after)) = (before, after) {
                if is_scriptio_continua(before) && is_scriptio_continua(after) {
                    continue;
                }
            }
        }
        output.push(ch);
    }

    output
}

/// Marks Windows resolves out of a letterbox edge or a border. Sentence
/// punctuation is deliberately absent: a trailing ？ or 。 is the subtitle.
const NOISE_MARKS: [char; 10] = ['“', '”', '„', '‟', '‘', '’', '‚', '‛', '«', '»'];

/// Whether a token is a single stray character of the kind noise resolves into.
fn is_stray_mark(token: &str) -> bool {
    let mut characters = token.chars();
    let Some(only) = characters.next() else {
        return false;
    };
    if characters.next().is_some() {
        return false;
    }
    only.is_ascii_digit() || only.is_ascii_punctuation() || NOISE_MARKS.contains(&only)
}

/// Whether a token belongs to a script written without spaces.
fn starts_scriptio_continua(token: &str) -> bool {
    token.chars().next().is_some_and(is_scriptio_continua)
}

/// Drop stray characters stranded at either end of a line.
///
/// Both conditions have to hold: the characters stand alone, and the dialogue
/// they are stranded against is CJK. That is what separates the bare "0"
/// Windows resolved beside Chinese dialogue from the 7 in "Chapter 7", and it
/// leaves a line that is genuinely just "0" alone, since at that point nothing
/// says it was noise.
///
/// A run is judged as a whole rather than one token at a time. Windows resolves
/// a letterbox edge into two marks as readily as one, and checking each against
/// its immediate neighbour meant the outer mark of "噁 0 0" was measured against
/// the inner one, found not to be CJK, and kept.
pub fn trim_edge_noise(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();

    let leading = tokens.iter().take_while(|t| is_stray_mark(t)).count();
    let trailing = tokens.iter().rev().take_while(|t| is_stray_mark(t)).count();

    // Nothing but marks: there is no dialogue to judge them against.
    if leading + trailing >= tokens.len() {
        return tokens.join(" ");
    }

    let start = if starts_scriptio_continua(tokens[leading]) {
        leading
    } else {
        0
    };
    let end = tokens.len() - trailing;
    let end = if starts_scriptio_continua(tokens[end - 1]) {
        end
    } else {
        tokens.len()
    };

    tokens[start..end.max(start)].join(" ")
}

/// Everything above, in the order the pipeline needs it: strip the stray
/// characters while they are still separate tokens, then close up the spacing.
pub fn clean_line(text: &str) -> String {
    join_scriptio_continua(&trim_edge_noise(text.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The read that started this: every glyph its own word, joined with spaces,
    // sent to the translator as if it were spaced-out prose.
    #[test]
    fn chinese_characters_are_joined_back_together() {
        assert_eq!(
            clean_line("亻 每 里 解 什 么 是 魔 术 师 之 间 的 斗 争 吗"),
            "亻每里解什么是魔术师之间的斗争吗"
        );
    }

    #[test]
    fn japanese_and_korean_are_joined_the_same_way() {
        assert_eq!(clean_line("魔 術 師 同 士"), "魔術師同士");
        assert_eq!(clean_line("안 녕 하 세 요"), "안녕하세요");
    }

    #[test]
    fn latin_word_spacing_survives() {
        assert_eq!(clean_line("Are you coming home?"), "Are you coming home?");
    }

    // A line with both scripts must keep the space that separates them.
    #[test]
    fn the_boundary_between_scripts_keeps_its_space() {
        assert_eq!(clean_line("Hello 世 界"), "Hello 世界");
        assert_eq!(clean_line("第 3 集"), "第 3 集");
    }

    // Windows resolved a letterbox edge into a bare digit, which reached the
    // viewer inside translated dialogue.
    #[test]
    fn a_stray_digit_at_either_end_is_dropped() {
        assert_eq!(clean_line("0 国 际 象 棋 之 类 的"), "国际象棋之类的");
        assert_eq!(
            clean_line("如 果 你 的 对 手 是 世 界 冠 军 0"),
            "如果你的对手是世界冠军"
        );
    }

    #[test]
    fn stray_punctuation_at_the_end_is_dropped() {
        assert_eq!(clean_line("有 没 有 别 的 办 法 “"), "有没有别的办法");
    }

    // A line that is only the stray character is left alone: at that point
    // there is nothing to say it was noise rather than the subtitle.
    #[test]
    fn a_line_that_is_only_a_digit_survives() {
        assert_eq!(clean_line("0"), "0");
        assert_eq!(clean_line("?"), "?");
        assert_eq!(clean_line("0 0"), "0 0");
    }

    // Windows resolves an edge into two marks as readily as one. Judging each
    // against its immediate neighbour left the outer one in place.
    #[test]
    fn a_run_of_stray_marks_at_either_end_is_dropped() {
        assert_eq!(clean_line("0 0 装 作 疯 了"), "装作疯了");
        assert_eq!(clean_line("走 噁 0 0"), "走噁");
        assert_eq!(clean_line("0 “ 你 来 了 吗 ？ 0 0"), "你来了吗？");
    }

    // A lone digit is only noise when it turned up beside CJK dialogue. Latin
    // numbering keeps its number.
    #[test]
    fn real_numbers_survive() {
        assert_eq!(clean_line("12 国 际 象 棋"), "12 国际象棋");
        assert_eq!(clean_line("Chapter 7"), "Chapter 7");
        assert_eq!(clean_line("Episode 3 of 5"), "Episode 3 of 5");
    }

    // Sentence punctuation is not noise, wherever it sits.
    #[test]
    fn trailing_sentence_punctuation_survives() {
        assert_eq!(clean_line("你 来 了 吗 ？"), "你来了吗？");
        assert_eq!(clean_line("我 明 白 了 。"), "我明白了。");
    }

    #[test]
    fn punctuation_inside_the_line_survives() {
        assert_eq!(clean_line("那 么 ， 你 呢 ？"), "那么，你呢？");
    }
}
