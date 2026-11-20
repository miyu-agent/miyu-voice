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

/// 连着 30 秒数字静音算卡住一次（该重开），头一回才说；有一点声音就重新数、说一声恢复了（施工 VO-26、VO-44）。
#[test]
fn every_thirty_seconds_of_digital_silence_is_stuck_but_said_once() {
    use super::{FRAME, Heard, SILENT_FOR, Silence};
    let block = vec![0.0f32; FRAME];
    let mut silence = Silence::default();
    let blocks = SILENT_FOR.div_ceil(FRAME);
    let heard: Vec<Heard> = (0..blocks * 3)
        .map(|_| silence.heard(&block))
        .filter(|heard| *heard != Heard::Quiet)
        .collect();
    assert_eq!(
        heard,
        [
            Heard::Stuck { first: true },
            Heard::Stuck { first: false },
            Heard::Stuck { first: false }
        ],
        "stuck every 30 s, said once"
    );
    assert_eq!(
        silence.heard(&[0.0, 0.2, 0.0]),
        Heard::Back,
        "a sound ends it"
    );
    assert_eq!(silence.heard(&[0.0, 0.2, 0.0]), Heard::Sound);
    let again: Vec<Heard> = (0..blocks)
        .map(|_| silence.heard(&block))
        .filter(|heard| *heard != Heard::Quiet)
        .collect();
    assert_eq!(
        again,
        [Heard::Stuck { first: true }],
        "said again for a new stretch"
    );
    let mut room = Silence::default();
    let noise: Vec<f32> = (0..FRAME)
        .map(|i| if i % 2 == 0 { 0.001 } else { -0.001 })
        .collect();
    assert!(
        (0..blocks * 2).all(|_| room.heard(&noise) == Heard::Sound),
        "a quiet room is not silent"
    );
}

/// 假的麦克风：每开一次记一笔、记下问到的设备；`silent_opens` 次以内开出来的一直吐全零，之后的吐底噪。开着的一个交满
/// 100 秒（一块 1 秒）就让整个程序退：一直不重开的，开一次就到头了。
struct FakeMicrophone {
    opens: usize,
    silent_opens: usize,
    quit: std::sync::Arc<std::sync::atomic::AtomicBool>,
    devices: Vec<Option<String>>,
    /// 一共交出去几块。
    delivered: std::rc::Rc<std::cell::Cell<usize>>,
}

impl FakeMicrophone {
    fn new(switches: &Switches, silent_opens: usize) -> FakeMicrophone {
        FakeMicrophone {
            opens: 0,
            silent_opens,
            quit: std::sync::Arc::clone(&switches.quit),
            devices: Vec::new(),
            delivered: std::rc::Rc::default(),
        }
    }
}

struct FakeInput {
    silent: bool,
    left: std::cell::Cell<usize>,
    quit: std::sync::Arc<std::sync::atomic::AtomicBool>,
    delivered: std::rc::Rc<std::cell::Cell<usize>>,
}

impl super::Microphone for FakeMicrophone {
    type Input = FakeInput;

    fn hold(&mut self, device: Option<&str>) -> super::Held<FakeInput> {
        self.opens += 1;
        self.devices.push(device.map(str::to_string));
        super::Held::Open(FakeInput {
            silent: self.opens <= self.silent_opens,
            left: std::cell::Cell::new(100),
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
        let left = self.left.get();
        if left == 0 {
            self.quit.store(true, Ordering::SeqCst);
            return None;
        }
        self.left.set(left - 1);
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

/// 常驻的录音一直全零（项目主人 11-20 验收第 4 条）：30 秒就关了重开、重新问设备，弹一次「麦克风没有声音」；重开以后有了声音
/// 弹「麦克风恢复了」。
#[test]
fn a_digitally_silent_microphone_is_reopened_and_said_once() {
    use crate::notify::Notice;
    let switches = Switches::default();
    let mut microphone = FakeMicrophone::new(&switches, 3);
    let notices = std::cell::RefCell::new(Vec::new());
    let say = |notice: Notice| notices.borrow_mut().push(notice);
    let mut frames = super::Frames::new(&switches, &say);
    let device = || Some("usb-mic".to_string());
    super::hold_and_listen(
        &mut microphone,
        &device,
        &switches,
        &mut frames,
        &mut |_| {},
    );
    assert_eq!(microphone.opens, 4, "reopened after each 30 s of zeros");
    assert_eq!(
        microphone.devices,
        vec![Some("usb-mic".to_string()); 4],
        "asks for the configured device each time"
    );
    assert_eq!(
        *notices.borrow(),
        [Notice::SilentMicrophone, Notice::MicrophoneBack],
        "said once, and again when sound came back"
    );
}

/// 设置页换了麦克风：拿着的这一个当场关了、照新的设备重开（施工 VO-44）。
#[test]
fn a_new_microphone_setting_reopens_the_microphone() {
    let switches = Switches::default();
    let mut microphone = FakeMicrophone::new(&switches, 0);
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
