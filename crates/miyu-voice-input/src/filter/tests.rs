use super::{clear, kept, long_enough};

#[test]
fn half_a_second_is_the_shortest() {
    assert!(!long_enough(499));
    assert!(long_enough(500));
}

#[test]
fn two_real_characters_are_clear() {
    assert!(clear("好的"));
    assert!(clear("ok"));
    assert!(!clear("嗯"), "one character");
    assert!(!clear("。，！ "), "only punctuation");
    assert!(!clear("嗯。"), "punctuation does not count");
}

#[test]
fn kept_trims_and_drops_unclear() {
    assert_eq!(kept("  今天开会。 ").as_deref(), Some("今天开会。"));
    assert_eq!(kept("啊"), None);
}
