//! 助手一直收着麦（设计 20 第九节）：助手在跑（包装着、没停用）、不是她在说话（第 7 步半双工）的时候，拿整机的麦克风锁、
//! 开麦，转成 16 kHz、一帧帧交给听的那一套。有听写要麦（终端、命令行点了麦克风，[`voice_common::mic_lock::MicLock::wanted`]）就关麦放锁，等它录完
//! 再拿（第九节「一个麦克风两个人用」）。她在说话的时候：开着「说话时可以打断 AI」的（默认）麦照样收，收到的先过回声消除
//! （[`crate::echo`]，施工 VO-20），不把她自己的声音当成你说的；关了的麦是关着的。
//!
//! 拿着麦就一直听，不按收到的声音大小判麦克风坏没坏（施工 VO-47，项目主人 2026-11-22 定：安静的房间、降噪过的麦克风本来就
//! 近乎全零，以前连着 30 秒全零就弹通知、关了重开，是误报）。设置页换了麦克风的当场关了重开（施工 VO-44）。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sherpa_onnx::LinearResampler;

use crate::echo::{Echo, Played};
use crate::listen::RATE;
use crate::notify::{Notice, Step};

mod microphone;
pub use microphone::{Held, Input, Microphone, System};

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
    /// 说话时可以打断 AI（默认开）：她说话的时候也收麦、过回声消除，好听得见打断（设计 20 第九节第 7 步）；关了的她说话时
    /// 不收麦。
    pub full_duplex: Arc<AtomicBool>,
    /// 麦克风里现在主要是她自己的声音（回声消除写）：这时听到的开口不算打断。
    pub mostly_echo: Arc<AtomicBool>,
    /// 音箱放过的声音：助手的音箱旁听了记在这，回声消除从这取。
    pub played: Played,
    /// 整个程序要退了。
    pub quit: Arc<AtomicBool>,
    /// 设置页换了麦克风：拿着的关了、照新的重开（施工 VO-44）。
    pub reopen: Arc<AtomicBool>,
}

impl Switches {
    fn wanted(&self) -> bool {
        self.full_duplex.load(Ordering::SeqCst) || !self.speaking.load(Ordering::SeqCst)
    }
}

/// 一直跑到 `quit`：该收的时候收，一帧帧交给 `frame`。`device` 交回现在配置的麦克风设备名（每次开麦时问，`None` 用默认的）；
/// `say` 弹给人看的通知。
///
/// 测试用：设了 `MIYU_VOICE_TEST_MIC`（一个 16 kHz 单声道的 wav）的不开麦，照实时的快慢把这个文件「听」一遍，前面垫
/// `MIYU_VOICE_TEST_MIC_LEAD` 秒（不设是两秒）、后面垫两秒安静，再往后一直是安静（端到端测试不碰主人的麦克风，施工 VO-11下、
/// VO-14）。另设了 `MIYU_VOICE_TEST_MIC_CUT_IN`（同样的 wav）的，
/// 她第一次开口念的时候把它「听」一遍，测开口就打断（施工 VO-12；关了「说话时可以打断 AI」的收不到）。
pub fn run(
    device: &dyn Fn() -> Option<String>,
    switches: &Switches,
    say: &dyn Fn(Notice),
    mut frame: impl FnMut(&[f32]),
) {
    let mut frames = Frames::new(switches, say);
    if let Some(path) = std::env::var_os("MIYU_VOICE_TEST_MIC") {
        play_file(
            &std::path::PathBuf::from(path),
            switches,
            &mut frames,
            &mut frame,
        );
        return;
    }
    hold_and_listen(&mut System, device, switches, &mut frames, &mut frame);
}

/// 拿麦、收、放麦，一直到 `quit`：开不了的过一会儿再试（同一句只说一次）；拿着的那一段完了（有人要麦、换了麦克风、设备
/// 出错）关了重开。
fn hold_and_listen<M: Microphone>(
    microphone: &mut M,
    device: &dyn Fn() -> Option<String>,
    switches: &Switches,
    frames: &mut Frames<'_>,
    frame: &mut impl FnMut(&[f32]),
) {
    let mut warned = None::<String>;
    while !switches.quit.load(Ordering::SeqCst) {
        if !switches.wanted() {
            std::thread::sleep(CHECK);
            continue;
        }
        switches.reopen.store(false, Ordering::SeqCst);
        let input = match microphone.hold(device().as_deref()) {
            Held::Open(input) => input,
            Held::Busy => {
                std::thread::sleep(RETRY);
                continue;
            }
            Held::Failed(why) => {
                // 同一句只说一次，免得没插麦的机器把日志写满。
                if warned.as_deref() != Some(why.as_str()) {
                    eprintln!("miyu-voice-assistant: {why}");
                    (frames.say)(Notice::Trouble(Step::Microphone, why.clone()));
                    warned = Some(why);
                }
                std::thread::sleep(RETRY * 4);
                continue;
            }
        };
        warned = None;
        listen_while_held(microphone, &input, switches, frames, frame);
    }
}

/// 收到的 16 kHz 的声音过回声消除、切成一帧帧。
struct Frames<'a> {
    echo: Echo,
    pending: Vec<f32>,
    say: &'a dyn Fn(Notice),
}

impl<'a> Frames<'a> {
    fn new(switches: &Switches, say: &'a dyn Fn(Notice)) -> Frames<'a> {
        Frames {
            echo: Echo::new(switches.played.clone(), Arc::clone(&switches.mostly_echo)),
            pending: Vec::new(),
            say,
        }
    }

    /// 收一块。
    fn feed(&mut self, heard: &[f32], frame: &mut impl FnMut(&[f32])) {
        self.pending.extend(self.echo.clean(heard));
        let whole = self.pending.len() / FRAME * FRAME;
        for piece in self.pending[..whole].chunks(FRAME) {
            frame(piece);
        }
        self.pending.drain(..whole);
    }
}

/// 拿着麦的这一段：收、转、消回声、切帧，直到有人要麦、关了、她开口（关了「说话时可以打断 AI」的）、设备出错、换了麦克风。
fn listen_while_held<M: Microphone>(
    microphone: &M,
    input: &M::Input,
    switches: &Switches,
    frames: &mut Frames<'_>,
    frame: &mut impl FnMut(&[f32]),
) {
    let rate = i32::try_from(input.rate()).unwrap_or(48_000);
    let resampler = (input.rate() as usize != RATE)
        .then(|| LinearResampler::create(rate, 16_000))
        .flatten();
    frames.pending.clear();
    let mut checked = Instant::now();
    loop {
        if !switches.wanted() || switches.quit.load(Ordering::SeqCst) {
            return;
        }
        if switches.reopen.load(Ordering::SeqCst) {
            eprintln!("miyu-voice-assistant: the microphone setting changed; reopening it");
            return;
        }
        // 有没有听写在等麦要查文件，隔一会儿查一次。
        if checked.elapsed() >= CHECK {
            checked = Instant::now();
            if microphone.wanted_elsewhere() {
                return;
            }
        }
        if let Some(why) = input.failure() {
            eprintln!("miyu-voice-assistant: {why}");
            return;
        }
        let Some(chunk) = input.next(Duration::from_millis(50)) else {
            continue;
        };
        match &resampler {
            Some(resampler) => frames.feed(&resampler.resample(&chunk, false), frame),
            None => frames.feed(&chunk, frame),
        }
    }
}

/// 测试用的「麦克风」：照实时的快慢；关了「说话时可以打断 AI」的，她在说话时不喂。
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
