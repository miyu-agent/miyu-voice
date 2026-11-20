use super::{Ears, Tuning};

fn tuning(words: &[&str], threshold: f32) -> Tuning {
    Tuning {
        wake_words: words.iter().map(|word| (*word).to_string()).collect(),
        threshold,
    }
}

/// 同一份配置只建一次；改了唤醒词、门槛的当场照新的重建（主人 11-20 验收第 3 条：改了唤醒词要重开助手才生效）。
#[test]
fn a_changed_wake_word_or_threshold_rebuilds_the_listener_at_once() {
    let mut ears = Ears::<Vec<String>>::default();
    let mut builds = 0;
    let mut build = |tuning: &Tuning| {
        builds += 1;
        Ok::<_, String>(tuning.wake_words.clone())
    };
    let first = tuning(&["你好你好"], 0.25);
    assert_eq!(
        ears.tuned(&first, &mut build).cloned(),
        Some(vec!["你好你好".to_string()])
    );
    for _ in 0..3 {
        assert!(ears.tuned(&first, &mut build).is_some());
    }
    let words = tuning(&["小美小美"], 0.25);
    assert_eq!(
        ears.tuned(&words, &mut build).cloned(),
        Some(vec!["小美小美".to_string()]),
        "the new wake word is heard without restarting"
    );
    assert!(
        ears.tuned(&tuning(&["小美小美"], 0.4), &mut build)
            .is_some()
    );
    assert_eq!(builds, 3, "once per distinct setting");
}

/// 建不成的（唤醒词一个都编不了、模型坏了）同一份配置不再试，改了再试。
#[test]
fn an_unbuildable_setting_is_tried_once_until_it_changes() {
    let mut ears = Ears::<()>::default();
    let mut tries = 0;
    let bad = tuning(&["?"], 0.25);
    for _ in 0..3 {
        let built = ears.tuned(&bad, |_| {
            tries += 1;
            Err("no wake word could be encoded".to_string())
        });
        assert!(built.is_none());
    }
    assert_eq!(tries, 1);
    assert!(
        ears.tuned(&tuning(&["你好你好"], 0.25), |_| Ok(()))
            .is_some()
    );
}
