// =============================================================================
// OCR_CORRUPTION.RS - how badly mangled a recognised line is
// =============================================================================
// One subtitle stays on screen for two to four seconds and is captured ten or
// more times. Windows OCR reads it differently each time, and the reads are not
// equally good: a 34-minute session recorded `where the Mystics have relatively
// thinned out?` and, one second later, `Wh€reythe MFtiÄhave relatively;
// thinnedput?` - the same cue, read once well and once as noise.
//
// The pipeline had no way to say which of those was worse, so the newest read
// always won and the viewer was left looking at the mangled one. Scoring the
// read is what lets a later read be refused - see issue #59.
//
// The score is deliberately comparative rather than absolute. Judging a line
// "bad enough to discard" needs a threshold that holds across every script and
// font the app might meet; judging one read worse than another read *of the same
// cue* needs only that the two be measured the same way, which is a far weaker
// claim and the one the fix actually rests on.
//
// The markers below were measured on English reads and apply only to them. A
// first version scored by token and let one bad character condemn a whole line,
// which is harmless in a script with spaces and catastrophic in one without:
// Chinese subtitles carry no spaces, so `他说:我们明天再谈吧` was a single token,
// the colon read as invented, and the line scored 1.0 and was thrown away. CJK
// corruption looks nothing like this anyway - it splits glyphs into radicals -
// so tokens carrying CJK are left alone rather than guessed at.
//
// The asymmetry that governs every threshold here: a mangled line that gets
// through is on screen until the next read a quarter-second later, while a real
// line rejected here is gone for good. Every rule is therefore set to under-
// report rather than over-report.
// =============================================================================

/// Characters that never appear inside a word in any script the app targets, so
/// finding one between two letters means OCR invented it.
///
/// Ordinary intra-word punctuation is excluded on purpose: an apostrophe in
/// `isn't`, a hyphen in `well-meaning` and a period in `U.S.` are all real, and
/// flagging them would score correct English worse than mangled English.
fn is_impossible_inside_a_word(ch: char) -> bool {
    matches!(
        ch,
        '€' | '£'
            | '¥'
            | '•'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '_'
            | '+'
            | '='
            | '|'
            | '\\'
            | '/'
            | '<'
            | '>'
            | '~'
            | '§'
            | '¤'
            | '©'
            | '®'
            | '°'
            | '±'
            | '¶'
            | '†'
            | '‡'
            | '˜'
            | 'Ł'
            | ';'
    )
}

/// Whether one whitespace-separated token looks like OCR noise rather than a word.
///
/// One marker only: a symbol wedged between two letters - `Wh€reythe`,
/// `qrßinary•Magebraft`, `R_4gng`, `bf//dzz`. It has to be *between* letters,
/// because subtitles carry standalone symbols (a dash for a speaker change,
/// `*whispering*`, `$100`) and those are real text.
///
/// A token carrying any CJK is never judged. The markers were measured on
/// English; Chinese is written without spaces, so a token is a whole line, and
/// `%`, `/` and Latin acronyms are ordinary content in one. See the header.
///
/// A second marker - two or more capitals after a token starts in lowercase -
/// was tried and removed. It was meant to catch `@lätiv.elYJthinned'.out`, whose
/// leading `@` this rule cannot see, but it also catches `iOS`, `macOS`, `eSIM`
/// and `mRNA`, and any all-caps line where OCR read an `I` as `l`. Those are
/// ordinary subtitle text, and losing one costs the whole line permanently.
fn is_garbled(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars
        .iter()
        .copied()
        .any(crate::llm::text_utils::is_cjk_char)
    {
        return false;
    }
    let embedded = chars.iter().enumerate().any(|(index, &ch)| {
        is_impossible_inside_a_word(ch)
            && index > 0
            && chars[..index]
                .iter()
                .any(|previous| previous.is_alphabetic())
            && chars[index + 1..].iter().any(|next| next.is_alphabetic())
    });
    embedded || starts_with_a_symbol_no_word_starts_with(&chars)
}

/// A symbol at the head of a token, from the few that never begin a real word.
///
/// `@lätiv.elYJthinned'.out` is the read this exists for: its only marker is the
/// leading `@`, which the embedded rule cannot see. The set is kept narrow
/// because leading symbols are usually real - `$100`, `#1`, `*whispering*`,
/// `-Where are you going?` - so only the ones no subtitle opens a word with are
/// listed.
fn starts_with_a_symbol_no_word_starts_with(chars: &[char]) -> bool {
    let opens = matches!(
        chars.first(),
        Some(
            '@' | '€'
                | '•'
                | '_'
                | '~'
                | '§'
                | '¤'
                | '©'
                | '®'
                | '±'
                | '¶'
                | '†'
                | '‡'
                | 'Ł'
                | '|'
        )
    );
    opens && chars[1..].iter().any(|next| next.is_alphabetic())
}

/// Fewest tokens a line needs before its noise share means anything.
///
/// Over one or two tokens a share is not a proportion but a coin flip: one
/// marker reads as 1.0 on a single-token line and as exactly the rejection
/// threshold on a two-token one. Either would discard a whole subtitle over one
/// suspicious character.
const MIN_TOKENS_TO_JUDGE: usize = 3;

/// How much of a line may be noise before translating it is pointless.
const MAX_CORRUPTION_SHARE: f32 = 0.5;

/// Whether a line is too mangled to be worth translating at all.
///
/// Harder to satisfy than `corruption_share > 0` on purpose. This is the only
/// place the score drops a line outright rather than ranking it against another
/// read of the same cue, so it is the only place where being wrong is permanent.
pub fn is_mostly_noise(text: &str) -> bool {
    text.split_whitespace().count() >= MIN_TOKENS_TO_JUDGE
        && corruption_share(text) >= MAX_CORRUPTION_SHARE
}

/// How much of a line is OCR noise, from 0.0 (clean) to 1.0 (entirely mangled).
///
/// Measured over tokens rather than characters so that one long mangled word
/// counts as much as one long clean word. A line with no tokens scores 0.0: an
/// empty read is handled by the gate, not here.
pub fn corruption_share(text: &str) -> f32 {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return 0.0;
    }
    let garbled = tokens.iter().filter(|token| is_garbled(token)).count();
    garbled as f32 / tokens.len() as f32
}

/// Whether `candidate` is a worse read than `current` of the same cue.
///
/// Only ever asked about two reads believed to be the same subtitle, so it does
/// not need to know what the cue says - only which reading of it is cleaner.
///
/// Ties go to the reading already on screen. A cue is re-read ten times, and
/// swapping between two equally good readings would flicker the line for no
/// gain; the viewer is already reading the one that is up.
pub fn is_worse_read(candidate: &str, current: &str) -> bool {
    let candidate_noise = corruption_share(candidate);
    let current_noise = corruption_share(current);
    if candidate_noise != current_noise {
        return candidate_noise > current_noise;
    }
    // Equally clean, so prefer the longer read: OCR drops glyphs and whole words
    // far more often than it invents them, and the recorded session's worst
    // replacements were fragments - `Mystics have` landing on top of the full
    // `where the Mystics have relatively thinned out?`.
    candidate.chars().count() < current.chars().count()
}

#[cfg(test)]
#[path = "ocr_corruption_tests.rs"]
mod tests;
