//! 识别放在子进程里（设计 20 第五节「省着用」）：主进程常驻、只占几百 KB 私有内存；要认的时候拉起
//! `miyu-voice-input recognize-worker`，它加载模型、认、空闲 [`IDLE`] 后自己退出，内存整份还给系统。
//!
//! 为什么不在主进程里卸：2026-11-09 实测，丢掉识别器以后常驻内存还剩 316～380 MB（glibc 把还回来的页留着），
//! 调 malloc 的门槛也还剩约 76 MB（onnxruntime 自己的全局东西）；子进程退出才是真放掉。代价是空闲后第一次认要重新加载
//! 模型（约半秒），和在主进程里懒加载一样。
//!
//! 两边一行一条 JSON：请求 `{"language", "wav"}`（`wav` 是 16 位 PCM wav 的 base64），回 `{"heard": {…}}` 或
//! `{"refused": {"kind", "message"}}`。

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use voice_common::audio::{self, Pcm};

use crate::model::Model;
use crate::recognize::{Heard, Recognizer, Refused};

/// 子进程空闲多久自己退（设计 20 第五节「空闲一会儿卸掉」）。
pub const IDLE: Duration = Duration::from_mins(5);

/// 一次识别至少给多久（加载模型、认短的一句）；长的另加音频时长的两倍（审计 #120：以前不限，子进程卡死就一直等）。
const ANSWER_WITHIN: Duration = Duration::from_mins(1);
/// 收子进程时关了它的输入以后等它自己退多久，过了就杀。
const EXIT_WITHIN: Duration = Duration::from_secs(2);

/// 主进程这一头：要认时拉起子进程，子进程退了下次再拉。一次认一个。
pub struct Workers {
    cache: PathBuf,
    running: Mutex<Option<Running>>,
    /// 子进程照什么拉起：平常是本程序加 `recognize-worker`，测试换成卡死的程序。
    command: Option<(PathBuf, Vec<String>)>,
    /// 至少等多久（测试调短）。
    within: Duration,
}

struct Running {
    child: Child,
    input: Option<BufWriter<ChildStdin>>,
    /// 子进程回的一行行：另一个线程读，这边带期限地等（[`ANSWER_WITHIN`]）。
    answers: mpsc::Receiver<String>,
}

/// 一次交换的结果。
enum Exchange {
    Answer(Result<Heard, Refused>),
    /// 管道断了：子进程退了或者死了，再拉一个试。
    Gone,
    /// 过了期限还没回：卡住了，杀掉，不再试。
    Stuck,
}

impl Workers {
    /// 模型在 `cache`（`MIYU_CACHE_DIR`）。
    #[must_use]
    pub fn new(cache: PathBuf) -> Workers {
        Workers {
            cache,
            running: Mutex::new(None),
            command: None,
            within: ANSWER_WITHIN,
        }
    }

    /// 测试用：子进程换成 `program args`，至少等 `within`（只有 Unix 上的测试用，拿 `sleep`、`true` 当子进程）。
    #[cfg(all(test, unix))]
    fn with_command(program: PathBuf, args: Vec<String>, within: Duration) -> Workers {
        Workers {
            cache: PathBuf::new(),
            running: Mutex::new(None),
            command: Some((program, args)),
            within,
        }
    }

    /// 交给子进程认。子进程刚好空闲退了的，重新拉起再试一次。
    ///
    /// # Errors
    ///
    /// 子进程认不了（太长、语言不对、模型坏了），或者拉不起来、半路死了、过了期限还没认完。
    pub fn transcribe(&self, pcm: &Pcm, language: &str) -> Result<Heard, Refused> {
        self.transcribe_wav(&audio::write_wav(&pcm.samples, pcm.rate), language)
    }

    /// 同 [`Workers::transcribe`]，交的是 wav 文件的字节：主进程不解码，原样交给子进程（施工 VO-9 实测：主进程解一段
    /// 两分多钟的录音，常驻涨到 47 MB、不还给系统）。
    ///
    /// # Errors
    ///
    /// 同上；读不出来的 wav 子进程照 `engine` 回。
    pub fn transcribe_wav(&self, wav: &[u8], language: &str) -> Result<Heard, Refused> {
        let request = json!({"language": language, "wav": STANDARD.encode(wav)}).to_string();
        // 16 kHz 单声道 16 位：一秒 32000 字节。
        let audio_secs = u64::try_from(wav.len() / 32_000).unwrap_or(u64::MAX);
        let within = self
            .within
            .saturating_add(Duration::from_secs(audio_secs.saturating_mul(2)));
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..2 {
            if running.is_none() {
                *running = Some(self.spawn()?);
            }
            let Some(worker) = running.as_mut() else {
                continue;
            };
            match exchange(worker, &request, within) {
                Exchange::Answer(answer) => return answer,
                // 退了或者死了：收掉，再拉一个。
                Exchange::Gone => drop(running.take()),
                Exchange::Stuck => {
                    drop(running.take());
                    return Err(Refused::Engine(
                        "The recognizer took too long and was stopped.".to_string(),
                    ));
                }
            }
        }
        Err(Refused::Engine(
            "The recognizer stopped unexpectedly.".to_string(),
        ))
    }

    fn spawn(&self) -> Result<Running, Refused> {
        let (program, args) = match &self.command {
            Some((program, args)) => (program.clone(), args.clone()),
            None => (
                std::env::current_exe().map_err(|error| {
                    Refused::Engine(format!("Cannot find this program: {error}."))
                })?,
                vec!["recognize-worker".to_string()],
            ),
        };
        let mut child = Command::new(program)
            .args(args)
            .env("MIYU_CACHE_DIR", &self.cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| Refused::Engine(format!("Cannot start the recognizer: {error}.")))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Refused::Engine("The recognizer has no pipes.".to_string()));
        };
        let (lines, answers) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if lines.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Running {
            child,
            input: Some(BufWriter::new(input)),
            answers,
        })
    }
}

impl Drop for Running {
    /// 主进程收掉子进程：关了它的标准输入，它读到头就退；[`EXIT_WITHIN`] 还没退的（卡死了）杀掉，再收。
    fn drop(&mut self) {
        drop(self.input.take());
        let deadline = std::time::Instant::now() + EXIT_WITHIN;
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_)) | Err(_)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 发一条、等一条，最多等 `within`。
fn exchange(worker: &mut Running, request: &str, within: Duration) -> Exchange {
    let Some(input) = worker.input.as_mut() else {
        return Exchange::Gone;
    };
    if writeln!(input, "{request}")
        .and_then(|()| input.flush())
        .is_err()
    {
        return Exchange::Gone;
    }
    match worker.answers.recv_timeout(within) {
        Ok(line) => match serde_json::from_str::<Value>(&line) {
            Ok(answer) => Exchange::Answer(decode(&answer)),
            Err(_) => Exchange::Gone,
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Exchange::Stuck,
        Err(mpsc::RecvTimeoutError::Disconnected) => Exchange::Gone,
    }
}

fn decode(answer: &Value) -> Result<Heard, Refused> {
    if let Some(heard) = answer.get("heard") {
        return Ok(Heard {
            text: heard["text"].as_str().unwrap_or_default().to_string(),
            language: heard["language"].as_str().unwrap_or_default().to_string(),
            duration_ms: heard["duration_ms"].as_u64().unwrap_or(0),
            clear: heard["clear"].as_bool().unwrap_or(false),
        });
    }
    let refused = &answer["refused"];
    let message = refused["message"].as_str().unwrap_or_default().to_string();
    Err(match refused["kind"].as_str() {
        Some("too_long") => Refused::TooLong,
        Some("language") => Refused::Language(message),
        _ => Refused::Engine(message),
    })
}

fn encode(answer: Result<Heard, Refused>) -> Value {
    match answer {
        Ok(heard) => json!({"heard": {
            "text": heard.text,
            "language": heard.language,
            "duration_ms": heard.duration_ms,
            "clear": heard.clear,
        }}),
        Err(Refused::TooLong) => json!({"refused": {"kind": "too_long", "message": ""}}),
        Err(Refused::Language(language)) => {
            json!({"refused": {"kind": "language", "message": language}})
        }
        Err(Refused::Engine(message)) => {
            json!({"refused": {"kind": "engine", "message": message}})
        }
    }
}

/// 子进程这一头（`recognize-worker`）：模型用时才加载，一条条认，空闲 [`IDLE`] 或者主进程关了管道就退。
pub fn serve(model: Model) {
    let (lines, incoming) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let recognizer = Recognizer::new(model);
    let mut out = std::io::stdout().lock();
    while let Ok(line) = incoming.recv_timeout(IDLE) {
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(request) => encode(handle(&recognizer, &request)),
            Err(error) => encode(Err(Refused::Engine(format!("Bad request: {error}.")))),
        };
        if writeln!(out, "{answer}")
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
}

fn handle(recognizer: &Recognizer, request: &Value) -> Result<Heard, Refused> {
    let wav = STANDARD
        .decode(request["wav"].as_str().unwrap_or_default())
        .map_err(|error| Refused::Engine(format!("Bad audio encoding: {error}.")))?;
    let pcm = audio::read_wav(&wav).map_err(|why| Refused::Engine(why.0))?;
    recognizer.transcribe(&pcm, request["language"].as_str().unwrap_or("zh"))
}

#[cfg(test)]
mod tests;
