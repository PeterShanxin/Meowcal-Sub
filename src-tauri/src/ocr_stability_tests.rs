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

// The headline instance from issue #59. The clean line was translated; the
// next read of the same cue wore a garbled prefix. It is not a second row
// arriving - it is the same line read worse - and retranslating it put
// `bf//dzz:: 不过...` over the good `不过...` 0.66 seconds later.
#[test]
fn a_re_read_wearing_a_garbled_prefix_is_not_extended() {
    assert_eq!(
        classify(
            "However, isn't he a hero from an era",
            "bf//dzz:: However, isn't he a hero from an era"
        ),
        LineChange::Repeat
    );
}

// The mirror of the above: real content appearing at the front of a line
// that was still drawing is new text, not noise - `where the Mystics have`
// arriving after the tail was read alone is worth retranslating.
#[test]
fn a_clean_prefix_growth_is_still_extended() {
    assert_eq!(
        classify(
            "relatively thinned out?",
            "where the Mystics have relatively thinned out?"
        ),
        LineChange::Extended
    );
}

// A prefix that carries a corruption marker is not automatically noise. An
// ampersand between letters is the marker that catches `bf//dzz`, and it is
// also how `R&D`, `AT&T` and `Q&A` are spelled. Judging the prefix by share
// let one such token outvote the clean words beside it, and the clause that
// had just appeared was never translated.
#[test]
fn genuine_prefix_growth_carrying_a_marker_is_still_extended() {
    for (previous, current) in [
        ("is hiring", "R&D department is hiring"),
        ("filed the paperwork", "AT&T lawyers filed the paperwork"),
        ("starts at seven", "the Q&A session starts at seven"),
    ] {
        assert_eq!(
            classify(previous, current),
            LineChange::Extended,
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
