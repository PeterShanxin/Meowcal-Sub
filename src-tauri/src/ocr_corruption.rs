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

/// Whether a run of text is noise all the way through, with no word in it.
///
/// Stricter than a share, and deliberately so. A share lets one marker-bearing
/// token outvote the clean words beside it, which is fine when the run is a
/// whole line being ranked and wrong when it is a two-token fragment being
/// judged outright: the marker that catches `bf//dzz` is the same ampersand that
/// spells `R&D`, `AT&T` and `Q&A`, so `R&D department` scored exactly the
/// rejection threshold and a real clause was thrown away.
///
/// Requiring every token to be garbled keeps the reads this was built for -
/// `bf//dzz::`, `Wh€reythe` - and lets any run containing an actual word
/// through. An empty run is not noise; it is nothing.
pub fn is_entirely_noise(text: &str) -> bool {
    let mut tokens = text.split_whitespace().peekable();
    tokens.peek().is_some() && tokens.all(is_garbled)
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

// There was an `is_worse_read` here, comparing two reads of one cue and keeping
// the cleaner. It is gone, and the note is worth more than the code was.
//
// A noisier re-read of the line on screen is classified `Repeat` and never
// reaches the translator - except the growth case, where a read that *contains*
// the previous line and adds to it is `Extended` and does reach the translator.
// A suffix is a second row arriving and must be translated (the only read that
// will ever carry it), but a garbled prefix is the same cue read worse. The
// caller that survived asked `is_worse_read` about `Extended` reads - which, by
// the definition in `ocr_stability`, *contain* the previous read. Scoring the
// whole candidate against its own prefix charged the newly arrived row's OCR
// noise to a baseline that never held that row, so any noise in the new text
// made the share strictly greater and the read was dropped. Dropped reads are
// never remembered, so the second row of a two-line cue would never appear at
// all.
//
// The garbled-prefix half of that gap is closed in `ocr_stability`:
// `wears_a_garbled_prefix` refuses an `Extended` read whose growth is a prefix
// carrying a corruption marker, so `bf//dzz:: However, isn't he a hero from an
// era` - the clearest instance in issue #59 - is a `Repeat` rather than a
// rival rendering. The suffix half remains deliberately open, because a suffix
// is how a second row arrives.
//
// See issue #59.

#[cfg(test)]
#[path = "ocr_corruption_tests.rs"]
mod tests;
