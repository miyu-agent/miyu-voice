//! 收麦的那一头（施工 VO-44）：真的是整机的麦克风锁加 cpal 开的麦克风（[`System`]），测试换成假的，好证明一直数字静音时会重开。

use std::time::Duration;

use voice_common::mic::Mic;
use voice_common::mic_lock::MicLock;

pub trait Microphone {
    /// 开着的一个输入（连着锁）：丢掉就关麦、放锁。
    type Input: Input;
    /// 拿锁、开 `device`（`None` 用系统默认的）。
    fn hold(&mut self, device: Option<&str>) -> Held<Self::Input>;
    /// 有听写在等麦（[`MicLock::wanted`]）：拿着的让出来。
    fn wanted_elsewhere(&self) -> bool;
}

/// 拿麦的结果。
pub enum Held<I> {
    /// 锁在别人手里（听写在录）：等一会儿再试。
    Busy,
    /// 开不了：为什么。
    Failed(String),
    /// 开好了。
    Open(I),
}

/// 一个开着的输入。
pub trait Input {
    /// 采样率。
    fn rate(&self) -> u32;
    /// 等最多 `wait`，交回收到的一块（单声道）。
    fn next(&self, wait: Duration) -> Option<Vec<f32>>;
    /// 设备半路出了错：为什么。
    fn failure(&self) -> Option<String>;
}

/// 这台电脑的麦克风：整机的麦克风锁、cpal。
pub struct System;

/// 开着的麦克风和拿着的锁。
pub struct Opened {
    mic: Mic,
    _lock: MicLock,
}

impl Microphone for System {
    type Input = Opened;

    fn hold(&mut self, device: Option<&str>) -> Held<Opened> {
        let Some(lock) = MicLock::for_assistant() else {
            return Held::Busy;
        };
        match Mic::open(device) {
            Ok(mic) => Held::Open(Opened { mic, _lock: lock }),
            Err(why) => Held::Failed(why),
        }
    }

    fn wanted_elsewhere(&self) -> bool {
        MicLock::wanted()
    }
}

impl Input for Opened {
    fn rate(&self) -> u32 {
        self.mic.rate
    }

    fn next(&self, wait: Duration) -> Option<Vec<f32>> {
        self.mic.chunks.recv_timeout(wait).ok()
    }

    fn failure(&self) -> Option<String> {
        self.mic.failure()
    }
}
