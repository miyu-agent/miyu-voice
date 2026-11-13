//! 助手一直收着麦（设计 20 第九节）：助手在跑（包装着、没停用）、不是她在说话（第 7 步半双工）的时候，拿整机的麦克风锁、
//! 开麦，转成 16 kHz、一帧帧交给听的那一套。有听写要麦（终端、命令行点了麦克风，[`MicLock::wanted`]）就关麦放锁，等它录完
//! 再拿（第九节「一个麦克风两个人用」）。她在说话的时候：开着「说话时可以打断她」的（默认）麦照样收，收到的先过回声消除
//! （[`crate::echo`]，施工 VO-20），不把她自己的声音当成你说的；关了的麦是关着的。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sherpa_onnx::LinearResampler;
use voice_common::mic::Mic;
use voice_common::mic_lock::MicLock;

use crate::echo::{Echo, Played};
use crate::listen::RATE;
use crate::notify::{Notice, Notifier, Step};

/// 一帧多少个采样（16 kHz 下 32 毫秒，同人声检测的窗）。
pub const FRAME: usize = 512;
/// 拿着麦的时候隔多久看一次有没有人要麦、还要不要听。
const CHECK: Duration = Duration::from_millis(200);
/// 没拿到麦（听写在用、开不了）时隔多久再试。
const RETRY: Duration = Duration::from_millis(500);

/// 什么时候该收麦。
#[derive(Clone, Default)]
pub struct Switches {
    /// `miyu listen` 开了对话（或热键）：收麦的线程下一帧开始等你开口。
    pub wake: Arc<AtomicBool>,
    /// 现在在对话里（叫醒了等下一句的、她在想在说的、追问窗口里的）：收麦的线程每一帧写，开关照它算现在开没开。
    pub open: Arc<AtomicBool>,
    /// 关了对话：收麦的线程把唤醒以后等下一句的那一段也收掉。
    pub ended: Arc<AtomicBool>,
    /// 对话是 `miyu listen off` 关的（已经响过、说过）：收麦的线程看到对话完了不再响、不再说。
    pub silenced: Arc<AtomicBool>,
    /// 她在说话。
    pub speaking: Arc<AtomicBool>,
    /// 说话时可以打断她（默认开）：她说话的时候也收麦、过回声消除，好听得见打断（设计 20 第九节第 7 步）；关了的她说话时
    /// 不收麦。
    pub full_duplex: Arc<AtomicBool>,
    /// 麦克风里现在主要是她自己的声音（回声消除写）：这时听到的开口不算打断。
    pub mostly_echo: Arc<AtomicBool>,
    /// 音箱放过的声音：助手的音箱旁听了记在这，回声消除从这取。
    pub played: Played,
    /// 整个程序要退了。
    pub quit: Arc<AtomicBool>,
}

impl Switches {
    fn wanted(&self) -> bool {
        self.full_duplex.load(Ordering::SeqCst) || !self.speaking.load(Ordering::SeqCst)
    }
}

/// 一直跑到 `quit`：该收的时候收，一帧帧交给 `frame`。`device` 是麦克风的设备名。
///
/// 测试用：设了 `MIYU_VOICE_TEST_MIC`（一个 16 kHz 单声道的 wav）的不开麦，照实时的快慢把这个文件「听」一遍，前面垫
/// `MIYU_VOICE_TEST_MIC_LEAD` 秒（不设是两秒）、后面垫两秒安静，再往后一直是安静（端到端测试不碰主人的麦克风，施工 VO-11下、
/// VO-14）。另设了 `MIYU_VOICE_TEST_MIC_CUT_IN`（同样的 wav）的，
/// 她第一次开口念的时候把它「听」一遍，测开口就打断（施工 VO-12；关了「说话时可以打断她」的收不到）。
pub fn run(
    device: Option<&str>,
    switches: &Switches,
    notifier: &Notifier,
    mut frame: impl FnMut(&[f32]),
) {
    let mut frames = Frames {
        echo: Echo::new(switches.played.clone(), Arc::clone(&switches.mostly_echo)),
        pending: Vec::new(),
        silence: Silence::default(),
        notifier,
    };
    if let Some(path) = std::env::var_os("MIYU_VOICE_TEST_MIC") {
        play_file(
            &std::path::PathBuf::from(path),
            switches,
            &mut frames,
            &mut frame,
        );
        return;
    }
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
                    notifier.show(Notice::Trouble(Step::Microphone, why.clone()));
                    warned = Some(why);
                }
                drop(lock);
                std::thread::sleep(RETRY * 4);
                continue;
            }
        };
        warned = None;
        listen_while_held(&mic, switches, &mut frames, &mut frame);
        drop(mic);
        drop(lock);
    }
}

/// 收到的 16 kHz 的声音过回声消除、切成一帧帧；连着 [`SILENT_FOR`] 都是数字静音的说一次（[`Silence`]）。
struct Frames<'a> {
    echo: Echo,
    pending: Vec<f32>,
    silence: Silence,
    notifier: &'a Notifier,
}

impl Frames<'_> {
    fn feed(&mut self, heard: &[f32], frame: &mut impl FnMut(&[f32])) {
        if self.silence.heard(heard) {
            eprintln!("miyu-voice-assistant: the microphone has been digitally silent for 30 s");
            self.notifier.show(Notice::SilentMicrophone);
        }
        self.pending.extend(self.echo.clean(heard));
        let whole = self.pending.len() / FRAME * FRAME;
        for piece in self.pending[..whole].chunks(FRAME) {
            frame(piece);
        }
        self.pending.drain(..whole);
    }
}

/// 连着多久都是数字静音才说（施工 VO-26）：正常的麦克风哪怕没人说话也有底噪，一直是零的是麦克风那头没声音。
const SILENT_FOR: usize = RATE * 30;

/// 数一数连着多少个样本是数字静音（[`voice_common::audio::silent`]）；到 [`SILENT_FOR`] 交回一次 `true`，有了声音重新数。
#[derive(Debug, Default)]
struct Silence {
    run: usize,
    said: bool,
}

impl Silence {
    fn heard(&mut self, samples: &[f32]) -> bool {
        if !voice_common::audio::silent(samples) {
            (self.run, self.said) = (0, false);
            return false;
        }
        self.run += samples.len();
        if self.run >= SILENT_FOR && !self.said {
            self.said = true;
            return true;
        }
        false
    }
}

/// 拿着麦的这一段：收、转、消回声、切帧，直到有人要麦、关了、她开口（关了「说话时可以打断她」的）、设备出错。
fn listen_while_held(
    mic: &Mic,
    switches: &Switches,
    frames: &mut Frames<'_>,
    frame: &mut impl FnMut(&[f32]),
) {
    let rate = i32::try_from(mic.rate).unwrap_or(48_000);
    let resampler = (mic.rate as usize != RATE)
        .then(|| LinearResampler::create(rate, 16_000))
        .flatten();
    frames.pending.clear();
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
            Some(resampler) => frames.feed(&resampler.resample(&chunk, false), frame),
            None => frames.feed(&chunk, frame),
        }
    }
}

/// 测试用的「麦克风」：照实时的快慢；关了「说话时可以打断她」的，她在说话时不喂。
fn play_file(
    path: &std::path::Path,
    switches: &Switches,
    frames: &mut Frames<'_>,
    frame: &mut impl FnMut(&[f32]),
) {
    let read = |path: &std::path::Path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| voice_common::audio::read_wav(&bytes).ok())
            .map(|pcm| pcm.samples)
            .unwrap_or_default()
    };
    let samples = read(path);
    let mut cut_in = std::env::var_os("MIYU_VOICE_TEST_MIC_CUT_IN")
        .map(|path| read(std::path::Path::new(&path)))
        .unwrap_or_default();
    let lead = std::env::var("MIYU_VOICE_TEST_MIC_LEAD")
        .ok()
        .and_then(|secs| secs.parse::<usize>().ok())
        .unwrap_or(2);
    let quiet = vec![0.0f32; RATE * 2];
    let mut audio = vec![0.0f32; RATE * lead];
    audio.extend(samples);
    audio.extend(&quiet);
    let mut at = 0;
    let silence = [0.0f32; FRAME];
    while !switches.quit.load(Ordering::SeqCst) {
        if !switches.wanted() {
            std::thread::sleep(CHECK);
            continue;
        }
        if at >= audio.len() && !cut_in.is_empty() && switches.speaking.load(Ordering::SeqCst) {
            audio.extend(std::mem::take(&mut cut_in));
        }
        let piece = audio.get(at..at + FRAME).unwrap_or(&silence);
        frames.feed(piece, frame);
        at += FRAME;
        std::thread::sleep(Duration::from_millis(32));
    }
}

#[cfg(test)]
mod tests;
