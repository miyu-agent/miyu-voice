//! 助手一直收着麦（设计 20 第九节）：开着（`miyu listen on`）、不是她在说话（第 7 步半双工）的时候，拿整机的麦克风锁、开麦，
//! 转成 16 kHz、一帧帧交给听的那一套。有听写要麦（终端、命令行点了麦克风，[`MicLock::wanted`]）就关麦放锁，等它录完再拿
//! （第九节「一个麦克风两个人用」）。关了、她在说话的时候麦是关着的：系统的麦克风指示灯不亮，也不收她自己的声音。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sherpa_onnx::LinearResampler;
use voice_common::mic::Mic;
use voice_common::mic_lock::MicLock;

use crate::listen::RATE;

/// 一帧多少个采样（16 kHz 下 32 毫秒，同人声检测的窗）。
pub const FRAME: usize = 512;
/// 拿着麦的时候隔多久看一次有没有人要麦、还要不要听。
const CHECK: Duration = Duration::from_millis(200);
/// 没拿到麦（听写在用、开不了）时隔多久再试。
const RETRY: Duration = Duration::from_millis(500);

/// 什么时候该收麦。
#[derive(Clone, Default)]
pub struct Switches {
    /// 开着（`miyu listen`）。
    pub listening: Arc<AtomicBool>,
    /// 她在说话（半双工：不收麦）。
    pub speaking: Arc<AtomicBool>,
    /// 整个程序要退了。
    pub quit: Arc<AtomicBool>,
}

impl Switches {
    fn wanted(&self) -> bool {
        self.listening.load(Ordering::SeqCst) && !self.speaking.load(Ordering::SeqCst)
    }
}

/// 一直跑到 `quit`：该收的时候收，一帧帧交给 `frame`。`device` 是麦克风的设备名。
pub fn run(device: Option<&str>, switches: &Switches, mut frame: impl FnMut(&[f32])) {
    let mut warned = None::<String>;
    while !switches.quit.load(Ordering::SeqCst) {
        if !switches.wanted() {
            std::thread::sleep(CHECK);
            continue;
        }
        let Some(lock) = MicLock::for_assistant() else {
            std::thread::sleep(RETRY);
            continue;
        };
        let mic = match Mic::open(device) {
            Ok(mic) => mic,
            Err(why) => {
                // 同一句只说一次，免得没插麦的机器把日志写满。
                if warned.as_deref() != Some(why.as_str()) {
                    eprintln!("miyu-voice-assistant: {why}");
                    warned = Some(why);
                }
                drop(lock);
                std::thread::sleep(RETRY * 4);
                continue;
            }
        };
        warned = None;
        listen_while_held(&mic, switches, &mut frame);
        drop(mic);
        drop(lock);
    }
}

/// 拿着麦的这一段：收、转、切帧，直到有人要麦、关了、她开口、设备出错。
fn listen_while_held(mic: &Mic, switches: &Switches, frame: &mut impl FnMut(&[f32])) {
    let rate = i32::try_from(mic.rate).unwrap_or(48_000);
    let resampler = (mic.rate as usize != RATE)
        .then(|| LinearResampler::create(rate, 16_000))
        .flatten();
    let mut pending: Vec<f32> = Vec::new();
    let mut checked = Instant::now();
    loop {
        if checked.elapsed() >= CHECK {
            checked = Instant::now();
            if MicLock::wanted() || !switches.wanted() || switches.quit.load(Ordering::SeqCst) {
                return;
            }
        }
        if let Some(why) = mic.failure() {
            eprintln!("miyu-voice-assistant: {why}");
            return;
        }
        let Ok(chunk) = mic.chunks.recv_timeout(Duration::from_millis(50)) else {
            continue;
        };
        match &resampler {
            Some(resampler) => pending.extend(resampler.resample(&chunk, false)),
            None => pending.extend(chunk),
        }
        let whole = pending.len() / FRAME * FRAME;
        for piece in pending[..whole].chunks(FRAME) {
            frame(piece);
        }
        pending.drain(..whole);
    }
}
