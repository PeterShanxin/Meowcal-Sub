use crate::ocr_corruption::*;

// The mangled reads and the clean lines they were mangled from are quoted from
// the session recorded in issue #59 (`meowcal-sub_2026-08-05_01-11-50.log`).
// The rest - ordinary words, standalone symbols, the Chinese lines - are chosen
// to pin the rules against text that session never contained, which is where
// the first version of this module went wrong.

// The clean reads. A rule that scores any of these as noise would throw away a
// correct subtitle, which is the failure this module must not cause.
#[test]
fn correctly_read_subtitles_score_clean() {
    for line in [
        "However, isn't he a hero from an era",
        "where the Mystics have relatively thinned out?",
        "Lionheart was called the \"WanderingKing!\"",
        "definitely beyond the confines Of ordinary Magecraft.",
        "Mystics have",
        "I do not know... maybe --- later",
        "如果想完全复刻再展开的话",
    ] {
        assert_eq!(corruption_share(line), 0.0, "{line:?} should score clean");
    }
}

// Words a weaker rule would have flagged. Real subtitles carry internal capitals
// and internal punctuation, and scoring them as noise costs a correct line.
#[test]
fn ordinary_words_with_internal_capitals_or_punctuation_score_clean() {
    for line in [
        "iPhone",
        "McCoy said so",
        "isn't well-meaning",
        "the U.S. delegation",
        "Mr. O'Brien-Smith",
    ] {
        assert_eq!(corruption_share(line), 0.0, "{line:?} should score clean");
    }
}

// The mangled reads, each of which was displayed to the viewer.
#[test]
fn mangled_reads_score_as_noise() {
    for line in [
        "Wh€reythe MFtiÄhave relatively;thinnedput?",
        "of qrßinary•Magebraft.",
        "R_4gng",
        "bf//dzz:: However, isn't he a hero from an era",
        "where the Mystics have @lätiv.elYJthinned'.out?",
    ] {
        assert!(
            corruption_share(line) > 0.0,
            "{line:?} should carry noise markers"
        );
    }
}

// The tests that stood here exercised `is_worse_read`, which has been removed -
// see the note in `ocr_corruption.rs`. They asserted the comparison was sound in
// isolation, and it was; what was unsound was every place it could be reached
// from. A test that pins a helper nothing may safely call is worse than no test,
// because it reads as coverage of a behaviour the app does not have.

// The symbol has to be *inside* a word. Subtitles carry standalone symbols -
// a dash for a speaker change, a musical note for lyrics - and those are real.
#[test]
fn a_symbol_standing_on_its_own_is_not_noise() {
    for line in ["- Where are you going?", "* whispering *", "100% certain"] {
        assert_eq!(corruption_share(line), 0.0, "{line:?} should score clean");
    }
}

#[test]
fn an_empty_or_blank_line_scores_clean_rather_than_panicking() {
    assert_eq!(corruption_share(""), 0.0);
    assert_eq!(corruption_share("   "), 0.0);
}

// The share is a proportion, so a line that is half noise scores half - the
// comparison in `is_worse_read` depends on it being graded rather than boolean.
#[test]
fn the_share_reflects_how_much_of_the_line_is_noise() {
    assert_eq!(corruption_share("R_4gng"), 1.0);
    assert_eq!(corruption_share("R_4gng clean words here"), 0.25);
}

// A known gap, recorded rather than hidden. `MFtiÄhave` is the module header's
// own example of a mangled read, and no marker here catches it: the accented
// capital is indistinguishable from a real one, and the rule that would have
// caught it also caught `iOS` and `mRNA`. It is ranked by length instead.
#[test]
fn a_mangled_word_with_no_symbol_is_not_detected() {
    assert_eq!(corruption_share("MFtiÄhave"), 0.0);
}

// Chinese is written without spaces, so a whole line is one token. Scoring it
// the way English is scored let one ordinary character - a colon, a slash, a
// percent sign, a Latin acronym - mark the entire subtitle as noise and have it
// thrown away by the gate. None of these lines carries any OCR damage at all.
#[test]
fn ordinary_chinese_subtitles_are_never_judged_as_noise() {
    for line in [
        "我们明天再谈吧",
        "他说:我们明天再谈吧",
        "股价上涨了30%以上真是难以置信",
        "第一季/第二季都很精彩",
        "他用iOS和Android系统",
        "我们明天•再谈吧",
    ] {
        assert_eq!(corruption_share(line), 0.0, "{line:?} should score clean");
        assert!(!is_mostly_noise(line), "{line:?} must not be rejected");
    }
}

// Lowercase-prefix acronyms and all-caps lines are ordinary subtitle text. An
// earlier rule counting capitals inside a word rejected every one of these.
#[test]
fn acronyms_and_all_caps_lines_are_not_noise() {
    for line in [
        "iOS",
        "mRNA",
        "eSIM",
        "macOS Ventura",
        "MAN:Hello there",
        "lNSPECTOR GADGET",
        "WARNlNG",
    ] {
        assert!(!is_mostly_noise(line), "{line:?} must not be rejected");
    }
}

// A share over one or two tokens is a coin flip rather than a proportion, so it
// cannot condemn a line on its own. Both of these carry a real marker; neither
// carries enough of the line to be sure.
#[test]
fn a_very_short_line_is_never_rejected_on_one_marker() {
    assert!(corruption_share("R_4gng") > 0.0);
    assert!(!is_mostly_noise("R_4gng"));
    assert!(!is_mostly_noise("of qrßinary•Magebraft."));
}

// `is_entirely_noise` judges a fragment outright rather than ranking it, so it
// has to be stricter than a share. The markers it looks for are the same ones
// that spell `R&D` and `AT&T`, and letting one of those outvote the real words
// beside it threw away a clause that had genuinely just appeared.
#[test]
fn only_a_fragment_with_no_word_in_it_is_entirely_noise() {
    for fragment in [
        "bf//dzz::",
        "Wh€reythe",
        "@lätiv.elYJthinned'.out",
        "bf//dzz:: Wh€reythe",
    ] {
        assert!(is_entirely_noise(fragment), "{fragment:?} is noise");
    }

    for fragment in [
        "R&D department",
        "AT&T lawyers",
        "the Q&A session",
        "However,",
        "",
        "   ",
    ] {
        assert!(
            !is_entirely_noise(fragment),
            "{fragment:?} contains a word and is not noise"
        );
    }
}
