use std::collections::HashSet;
use std::fmt::Write;

use super::{encode, tokens};

fn vocabulary(units: &[&str]) -> HashSet<String> {
    let mut text = String::new();
    for (n, unit) in units.iter().enumerate() {
        writeln!(text, "{unit} {n}").unwrap();
    }
    tokens(&text)
}

#[test]
fn characters_become_initial_and_final_pairs() {
    let vocab = vocabulary(&["w", "èi", "y", "ǒu"]);
    assert_eq!(
        encode("未有未有", &vocab).unwrap(),
        ["w èi y ǒu w èi y ǒu @未有未有"]
    );
}

#[test]
fn a_zero_initial_syllable_is_one_unit() {
    let vocab = vocabulary(&["ài", "x", "iǎo"]);
    assert_eq!(encode("小爱", &vocab).unwrap(), ["x iǎo ài @小爱"]);
}

#[test]
fn latin_expands_into_tone_candidates() {
    let vocab = vocabulary(&["m", "y", "i", "u", "ī", "í", "ǐ", "ì", "ū", "ú", "ǔ", "ù"]);
    let lines = encode("miyumiyu", &vocab).unwrap();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert_eq!(lines[0], "m ī y ū m ī y ū @miyumiyu");
    assert_eq!(lines[4], "m i y u m i y u @miyumiyu");
}

#[test]
fn kana_reads_like_latin_and_katakana_like_hiragana() {
    let vocab = vocabulary(&["m", "y", "ī", "ū"]);
    // 词表里只有一声：别的声调的候选编不出来，静静跳过。
    assert_eq!(
        encode("みゆみゆ", &vocab).unwrap(),
        ["m ī y ū m ī y ū @みゆみゆ"]
    );
    assert_eq!(
        encode("ミユミユ", &vocab).unwrap()[0],
        "m ī y ū m ī y ū @ミユミユ"
    );
}

#[test]
fn small_kana_join_and_double_consonants_drop() {
    let vocab = vocabulary(&["x", "iā", "iū", "q", "k", "ā", "t"]);
    assert_eq!(encode("しゃ", &vocab).unwrap()[0], "x iā @しゃ");
    assert_eq!(encode("きゅ", &vocab).unwrap()[0], "q iū @きゅ");
    assert_eq!(encode("かった", &vocab).unwrap()[0], "k ā t ā @かった");
}

#[test]
fn written_pinyin_is_taken_as_written() {
    let vocab = vocabulary(&["m", "ì", "y", "ǒu"]);
    assert_eq!(encode("mi4 you3", &vocab).unwrap(), ["m ì y ǒu @mi4_you3"]);
}

#[test]
fn other_characters_and_unknown_syllables_are_refused() {
    let vocab = vocabulary(&["m", "ì"]);
    assert!(encode("", &vocab).is_err());
    assert!(encode("hey!", &vocab).unwrap_err().0.contains('!'));
    assert!(encode("你好", &vocab).unwrap_err().0.contains("vocabulary"));
}

/// 训令式、外来音的音节也能切（审计 #125）：拉丁字母写的 `tina`、`sisi` 不报错。
#[test]
fn kunrei_and_loanword_syllables_split() {
    let vocab = vocabulary(&["t", "ī", "n", "ā", "s"]);
    assert_eq!(encode("tina", &vocab).unwrap()[0], "t ī n ā @tina");
    assert_eq!(encode("sisi", &vocab).unwrap()[0], "s ī s ī @sisi");
}

/// 小写元音和前一个音节拼（审计 #125）：ティ 是 ti 不是 te，フィ 是 fi，ウィ 是 wi；ヴ 认得。
#[test]
fn small_vowels_join_the_syllable_before() {
    let vocab = vocabulary(&["t", "ī", "n", "ā", "f", "ēi", "w", "sh", "ē"]);
    assert_eq!(encode("ティナ", &vocab).unwrap()[0], "t ī n ā @ティナ");
    assert_eq!(encode("フィー", &vocab).unwrap()[0], "f ēi @フィー");
    assert_eq!(encode("ヴァ", &vocab).unwrap()[0], "w ā @ヴァ");
    assert_eq!(encode("ウィ", &vocab).unwrap()[0], "w ēi @ウィ");
    assert_eq!(encode("シェ", &vocab).unwrap()[0], "sh ē @シェ");
}
