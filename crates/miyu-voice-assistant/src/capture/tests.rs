use std::sync::atomic::Ordering;

use super::Switches;

/// 一直收麦（施工 VO-14：开关管的是对话，不管听不听唤醒词）；关了「说话时可以打断 AI」的她说话时不收，开着的照收；开着对话不改收不收。
#[test]
fn the_microphone_is_wanted_unless_she_speaks_without_headphones() {
    let switches = Switches::default();
    assert!(switches.wanted(), "running means listening");
    switches.speaking.store(true, Ordering::SeqCst);
    assert!(!switches.wanted(), "half duplex: not while she speaks");
    switches.full_duplex.store(true, Ordering::SeqCst);
    assert!(
        switches.wanted(),
        "full duplex: keep listening to hear a cut-in"
    );
    switches.wake.store(true, Ordering::SeqCst);
    switches.full_duplex.store(false, Ordering::SeqCst);
    assert!(
        !switches.wanted(),
        "a conversation started by miyu listen still waits until she stops"
    );
}

/// 假的麦克风：每开一次记一笔、记下问到的设备；`silent` 的一直吐全零，不然吐底噪。一共交满 100 秒（一块 1 秒）就让整个程序
/// 退，开了几次都算在一起。
struct FakeMicrophone {
    opens: usize,
    silent: bool,
    quit: std::sync::Arc<std::sync::atomic::AtomicBool>,
    devices: Vec<Option<String>>,
    /// 一共交出去几块。
    delivered: std::rc::Rc<std::cell::Cell<usize>>,
}

impl FakeMicrophone {
    fn new(switches: &Switches, silent: bool) -> FakeMicrophone {
        FakeMicrophone {
            opens: 0,
            silent,
            quit: std::sync::Arc::clone(&switches.quit),
            devices: Vec::new(),
            delivered: std::rc::Rc::default(),
        }
    }
}

struct FakeInput {
    silent: bool,
    quit: std::sync::Arc<std::sync::atomic::AtomicBool>,
    delivered: std::rc::Rc<std::cell::Cell<usize>>,
}

impl super::Microphone for FakeMicrophone {
    type Input = FakeInput;

    fn hold(&mut self, device: Option<&str>) -> super::Held<FakeInput> {
        self.opens += 1;
        self.devices.push(device.map(str::to_string));
        super::Held::Open(FakeInput {
            silent: self.silent,
            quit: std::sync::Arc::clone(&self.quit),
            delivered: std::rc::Rc::clone(&self.delivered),
        })
    }

    fn wanted_elsewhere(&self) -> bool {
        false
    }
}

impl super::Input for FakeInput {
    fn rate(&self) -> u32 {
        16_000
    }

    fn next(&self, _: std::time::Duration) -> Option<Vec<f32>> {
        if self.delivered.get() >= 100 {
            self.quit.store(true, Ordering::SeqCst);
            return None;
        }
        self.delivered.set(self.delivered.get() + 1);
        Some(if self.silent {
            vec![0.0; 16_000]
        } else {
            (0..16_000)
                .map(|i| if i % 2 == 0 { 0.001 } else { -0.001 })
                .collect()
        })
    }

    fn failure(&self) -> Option<String> {
        None
    }
}

/// 一直全零的麦克风也一直开着听（施工 VO-47，项目主人 2026-11-22 定：安静的房间、降噪过的麦克风本来就近乎全零）：只开一次、
/// 100 秒都交给听的那一套，不弹通知。
#[test]
fn a_digitally_silent_microphone_stays_open_without_a_notice() {
    use crate::notify::Notice;
    let switches = Switches::default();
    let mut microphone = FakeMicrophone::new(&switches, true);
    let notices = std::cell::RefCell::new(Vec::<Notice>::new());
    let say = |notice: Notice| notices.borrow_mut().push(notice);
    let mut frames = super::Frames::new(&switches, &say);
    let device = || None;
    let mut heard = 0;
    super::hold_and_listen(
        &mut microphone,
        &device,
        &switches,
        &mut frames,
        &mut |_| heard += 1,
    );
    assert_eq!(microphone.opens, 1, "never reopened");
    assert_eq!(microphone.delivered.get(), 100);
    assert_eq!(
        heard,
        100 * 16_000 / super::FRAME,
        "every frame is listened to"
    );
    assert!(notices.borrow().is_empty(), "{:?}", notices.borrow());
}

/// 设置页换了麦克风：拿着的这一个当场关了、照新的设备重开（施工 VO-44）。
#[test]
fn a_new_microphone_setting_reopens_the_microphone() {
    let switches = Switches::default();
    let mut microphone = FakeMicrophone::new(&switches, false);
    let say = |_| {};
    let mut frames = super::Frames::new(&switches, &say);
    let reopen = std::sync::Arc::clone(&switches.reopen);
    let asked = std::cell::Cell::new(0);
    let device = || {
        asked.set(asked.get() + 1);
        // 第一次开麦的时候，设置页正好换了麦克风。
        if asked.get() == 1 {
            reopen.store(true, Ordering::SeqCst);
            Some("old".to_string())
        } else {
            Some("new".to_string())
        }
    };
    super::hold_and_listen(
        &mut microphone,
        &device,
        &switches,
        &mut frames,
        &mut |_| {},
    );
    assert_eq!(
        microphone.devices,
        [Some("old".to_string()), Some("new".to_string())]
    );
    assert_eq!(
        microphone.delivered.get(),
        100,
        "the old one is closed before it hands anything over"
    );
}
