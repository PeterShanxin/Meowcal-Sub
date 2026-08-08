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
    let previous_norm = normalize(previous);
    let current_norm = normalize(current);

    if previous_norm.is_empty() {
        return LineChange::New;
    }
    if previous_norm == current_norm {
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
    if contains_subsequence(&previous_norm, &current_norm)
        && current_norm.len() * 2 >= previous_norm.len()
    {
        return LineChange::Repeat;
    }

    // Growth is only worth retranslating when it carries a clause. A stray
    // glyph resolved at either edge of the strip grows the read too, and
    // retranslating on it swaps a good line on screen for a different one.
    if contains_subsequence(&current_norm, &previous_norm) {
        return if current_norm.len() - previous_norm.len() >= EXTENDED_MIN_NEW_CHARS
            && !wears_a_garbled_prefix(previous, current)
        {
            LineChange::Extended
        } else {
            LineChange::Repeat
        };
    }

    if previous_norm.len().max(current_norm.len()) >= MIN_CHARS_FOR_SIMILARITY
        && similarity(&previous_norm, &current_norm) >= SAME_LINE_SIMILARITY
    {
        return LineChange::Repeat;
    }

    LineChange::New
}

/// Whether the read is the previous line wearing a garbled prefix - the
/// clearest instance in issue #59, where a clean `However, isn't he a hero
/// from an era` was followed a frame later by `bf//dzz:: However, isn't he a
/// hero from an era` and the second, worse read replaced the good translation
/// on screen.
///
/// Growth here is not a second row arriving; the previous line is still
/// present in full, and the new characters are OCR noise prepended to it. The
/// marker is in the raw text (the `//`), so this must see the raw strings,
/// not the normalised rows the rest of `classify` works on - normalising
/// strips exactly the punctuation that proves it.
///
/// Only a prefix is judged. A second row appends at the end, and a suffix that
/// arrives carrying its own noise is still the only read that will ever
/// contain that row - see `ocr_recent_lines`.
///
/// The prefix has to be noise *all the way through*. Scoring it by share let one
/// marker-bearing token outvote clean words beside it, and the marker that
/// catches `bf//dzz` is the same ampersand that spells `R&D` and `AT&T`: a
/// prefix like `R&D department ` scored exactly the rejection threshold, so a
/// clause that had genuinely just appeared was suppressed and never translated.
fn wears_a_garbled_prefix(previous: &str, current: &str) -> bool {
    if !current.ends_with(previous) {
        return false;
    }
    let prefix = &current[..current.len() - previous.len()];
    crate::ocr_corruption::is_entirely_noise(prefix)
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
#[path = "ocr_stability_tests.rs"]
mod tests;
